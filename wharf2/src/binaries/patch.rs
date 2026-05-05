use super::{Dump, LendingIterator, WharfBinary};
use crate::decompress::Decompressor;
use crate::errors::{InconsistentMessage, IoError, Result};
use crate::magic::PATCH_MAGIC;
use crate::protos::sync_header::Type;
use crate::protos::{BsdiffHeader, Container, Control, Message, PatchHeader, SyncHeader, SyncOp};

use std::fmt::Display;
use std::io::{BufRead, Read, Write};
use std::iter::FusedIterator;
use std::marker::PhantomData;
use std::ops::Range;

mod op_kind {
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub struct Rsync;
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub struct Bsdiff;
}

/// Iterator over the patch operations ([`RsyncOp`] or [`BsdiffOp`]) for a single file
///
/// The kind of operations yielded is determined by the `K` type parameter:
/// [`op_kind::Rsync`] yields [`RsyncOp`]s terminated by a `HeyYouDidIt`
/// sentinel, and [`op_kind::Bsdiff`] yields [`BsdiffOp`]s terminated by an
/// `Eof` control message followed by a `HeyYouDidIt` sentinel.
///
/// Construct via [`PatchOpIter::rsync`], [`PatchOpIter::bsdiff`], or
/// [`PatchOpIter::empty`].
pub struct PatchOpIter<R: Read, K> {
  reader: R,
  has_finished: bool,
  _kind: PhantomData<K>,
}

impl<R: Read> PatchOpIter<R, op_kind::Rsync> {
  fn empty(reader: R) -> Self {
    Self {
      reader,
      has_finished: true,
      _kind: PhantomData,
    }
  }

  fn rsync(reader: R) -> Self {
    Self {
      reader,
      has_finished: false,
      _kind: PhantomData,
    }
  }
}

impl<R: Read> PatchOpIter<R, op_kind::Bsdiff> {
  fn bsdiff(reader: R) -> Self {
    Self {
      reader,
      has_finished: false,
      _kind: PhantomData,
    }
  }
}

impl<R: Read, K, T> PatchOpIter<R, K>
where
  PatchOpIter<R, K>: Iterator<Item = Result<T>>,
{
  fn drain(&mut self) -> Result<()> {
    for op in self {
      op?;
    }

    Ok(())
  }
}

impl<R: Read, K, T> Dump for PatchOpIter<R, K>
where
  PatchOpIter<R, K>: Iterator<Item = Result<T>>,
  T: Dump,
{
  fn dump(&mut self, writer: &mut impl Write) -> Result<()> {
    for op in self {
      op?.dump(writer)?;
    }

    Ok(())
  }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RsyncOp {
  BlockRange {
    file_index: usize,
    block_index: u64,
    block_span: u64,
  },
  Data(Box<[u8]>),
}

impl From<SyncOp> for RsyncOp {
  fn from(value: SyncOp) -> Self {
    match value {
      SyncOp::BlockRange {
        file_index,
        block_index,
        block_span,
      } => Self::BlockRange {
        file_index,
        block_index,
        block_span,
      },
      SyncOp::Data(data) => Self::Data(data),
      SyncOp::HeyYouDidIt => unreachable!(),
    }
  }
}

impl Dump for RsyncOp {
  fn dump(&mut self, writer: &mut impl Write) -> Result<()> {
    writeln!(writer, "{:?}", self).map_err(|e| IoError::WriteDumpFailed(e).into())
  }
}

impl<R: Read> Iterator for PatchOpIter<R, op_kind::Rsync> {
  type Item = Result<RsyncOp>;

  /// Decode the next [`SyncOp`] in the stream
  fn next(&mut self) -> Option<Self::Item> {
    if self.has_finished {
      return None;
    }

    Some(match SyncOp::decode(&mut self.reader) {
      Ok(SyncOp::HeyYouDidIt) => {
        self.has_finished = true;
        return None;
      }
      Ok(sync_op) => Ok(sync_op.into()),
      Err(e) => Err(e),
    })
  }
}

impl<R: Read, K> FusedIterator for PatchOpIter<R, K> where PatchOpIter<R, K>: Iterator {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BsdiffOp {
  pub add: Box<[u8]>,
  pub copy: Box<[u8]>,
  pub seek: i64,
}

impl From<Control> for BsdiffOp {
  fn from(value: Control) -> Self {
    match value {
      Control::Op { add, copy, seek } => Self { add, copy, seek },
      Control::Eof => unreachable!(),
    }
  }
}

impl Dump for BsdiffOp {
  fn dump(&mut self, writer: &mut impl Write) -> Result<()> {
    writeln!(writer, "{:?}", self).map_err(|e| IoError::WriteDumpFailed(e).into())
  }
}

impl<R: Read> Iterator for PatchOpIter<R, op_kind::Bsdiff> {
  type Item = Result<BsdiffOp>;

  /// Decode the next [`Control`] in the stream
  fn next(&mut self) -> Option<Self::Item> {
    if self.has_finished {
      return None;
    }

    Some(match Control::decode(&mut self.reader) {
      Ok(Control::Eof) => {
        // After the bsdiff EOF message, a rsync HeyYouDidIt message follows
        match SyncOp::decode(&mut self.reader) {
          Ok(SyncOp::HeyYouDidIt) => (),
          Ok(_) => {
            return Some(Err(
              InconsistentMessage::ExpectedHeyYouDidIt.into_error::<SyncOp>(),
            ));
          }
          Err(e) => return Some(Err(e)),
        }

        self.has_finished = true;
        return None;
      }
      Ok(sync_op) => Ok(sync_op.into()),
      Err(e) => Err(e),
    })
  }
}

/// The patch operation kind for a single file, holding the concrete op iterator
///
/// - `Rsync`: the file was patched using rsync block operations and raw data chunks.
/// - `Bsdiff`: the file was patched using bsdiff binary diff controls against
///   the old file at `target_index` in the old container.
pub enum PatchOp<R: Read> {
  Rsync {
    iter: PatchOpIter<R, op_kind::Rsync>,
  },
  Bsdiff {
    iter: PatchOpIter<R, op_kind::Bsdiff>,
    #[expect(dead_code)]
    target_index: usize,
  },
}

impl<R: Read> PatchOp<R> {
  fn drain(&mut self) -> Result<()> {
    match self {
      Self::Rsync { iter } => iter.drain(),
      Self::Bsdiff { iter, .. } => iter.drain(),
    }
  }

  fn reader(self) -> R {
    match self {
      Self::Rsync { iter } => iter.reader,
      Self::Bsdiff { iter, .. } => iter.reader,
    }
  }

  fn empty(reader: R) -> Self {
    Self::Rsync {
      iter: PatchOpIter::empty(reader),
    }
  }

  fn rsync(reader: R) -> Self {
    Self::Rsync {
      iter: PatchOpIter::rsync(reader),
    }
  }

  fn bsdiff(reader: R, target_index: usize) -> Self {
    Self::Bsdiff {
      iter: PatchOpIter::bsdiff(reader),
      target_index,
    }
  }
}

impl<R: Read> Dump for PatchOp<R> {
  fn dump(&mut self, writer: &mut impl Write) -> Result<()> {
    match self {
      PatchOp::Rsync { iter } => iter.dump(writer),
      PatchOp::Bsdiff { iter, .. } => iter.dump(writer),
    }
  }
}

/// Lending iterator over per-file [`PatchOp`]s in a patch stream
///
/// Advances through the new container's files in order, yielding a
/// [`PatchOp`] for each file. When [`LendingIterator::next`] is called, any
/// unread operations from the previous file are drained to keep the underlying
/// reader in sync, then the next [`SyncHeader`] is decoded to determine the
/// kind of operations for the current file.
pub struct FilePatchIter<R: Read> {
  // The patch iter is stored as an Option to allow replacing the Rsync kind
  // with the Bsdiff kind iter by taking the reader out
  patch_iter: Option<PatchOp<R>>,

  file_indexes: Range<usize>,
}

impl<R: Read> FilePatchIter<R> {
  fn new(reader: R, container: &Container) -> Self {
    // Create the inner patch iter
    let patch_iter = PatchOp::empty(reader);

    Self {
      patch_iter: Some(patch_iter),
      file_indexes: 0..container.files.len(),
    }
  }
}

impl<R: Read> LendingIterator for FilePatchIter<R> {
  type Item<'a>
    = Result<(usize, &'a mut PatchOp<R>)>
  where
    R: 'a;

  fn next<'a>(&'a mut self) -> Option<Self::Item<'a>> {
    let file_index = self.file_indexes.next()?;

    // Take the patch iter out of the Option
    // It is very important to put it back into place before returning
    let mut patch_iter = self.patch_iter.take().unwrap();

    // Skip the patch operations that belong to the last file and have not been read
    if let Err(e) = patch_iter.drain() {
      return Some(Err(e));
    }

    let mut reader = patch_iter.reader();

    // Determine the kind of patch operations for this file
    let header = match SyncHeader::decode(&mut reader) {
      Ok(header) => header,
      Err(e) => return Some(Err(e)),
    };

    // The index provided in the header must be equal to file_index.
    if header.file_index != file_index {
      return Some(Err(
        InconsistentMessage::NonconsecutivePatchHeaderIndex {
          expected: file_index,
          found: header.file_index,
        }
        .into_error::<SyncHeader>(),
      ));
    }

    let patch_iter = self.patch_iter.insert(match header.r#type {
      Type::Rsync => PatchOp::rsync(reader),
      Type::Bsdiff => {
        // Decode the bsdiff header
        let bsdiff_header = match BsdiffHeader::decode(&mut reader) {
          Ok(header) => header,
          Err(e) => return Some(Err(e)),
        };

        PatchOp::bsdiff(reader, bsdiff_header.target_index)
      }
    });

    Some(Ok((file_index, patch_iter)))
  }
}

impl<R: Read> Dump for FilePatchIter<R> {
  fn dump(&mut self, writer: &mut impl Write) -> Result<()> {
    while let Some(item) = self.next() {
      match item {
        Ok((_file_index, patch_iter)) => patch_iter.dump(writer)?,
        Err(e) => return Err(e),
      }
    }

    Ok(())
  }
}

/// A wharf patch file (`.pwr`)
///
/// A patch contains the old and new containers describing the filesystem state
/// before and after the patch, followed by a sequence of per-file patch
/// operations. Each file in the new container is either an rsync or bsdiff
/// patch.
///
/// # References
///
/// <https://docs.itch.zone/wharf/master/file-formats/patches.html>
pub struct Patch<'reader, R: BufRead> {
  header: PatchHeader,
  container_old: Container,
  container_new: Container,
  patch_iter: FilePatchIter<Decompressor<'reader, R>>,
}

impl<R: BufRead> Display for Patch<'_, R> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "wharf patch file ({})
  old: {}
  new: {}",
      self.header.compression, self.container_old, self.container_new
    )
  }
}

impl<R: BufRead> Dump for Patch<'_, R> {
  fn dump(&mut self, writer: &mut impl Write) -> Result<()> {
    self.header.dump(writer)?;
    self.container_old.dump(writer)?;
    self.container_new.dump(writer)?;
    self.patch_iter.dump(writer)
  }
}

impl<'reader, R: BufRead + 'reader> WharfBinary<'reader, R> for Patch<'reader, R> {
  const MAGIC: u32 = PATCH_MAGIC;

  fn read_without_magic(reader: &'reader mut R) -> Result<Self> {
    // Decode the patch header
    let header = PatchHeader::decode(reader)?;

    // Decompress the remaining stream
    let mut reader = Decompressor::new(reader, header.compression)?;

    // Decode the old container
    let container_old = Container::decode(&mut reader)?;

    // Decode the new container
    let container_new = Container::decode(&mut reader)?;

    // Create a new patch iter
    let patch_iter = FilePatchIter::new(reader, &container_new);

    Ok(Patch {
      header,
      container_old,
      container_new,
      patch_iter,
    })
  }
}
