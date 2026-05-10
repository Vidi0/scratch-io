use super::{Dump, LendingIterator, WharfBinary};
use crate::decompress::Decompressor;
use crate::errors::{InconsistentMessage, IoError, Result};
use crate::magic::PATCH_MAGIC;
use crate::protos::sync_header::Type;
use crate::protos::{BsdiffHeader, Container, Control, Message, PatchHeader, SyncHeader, SyncOp};

use std::fmt::Display;
use std::io::{BufRead, Read, Write};
use std::iter::FusedIterator;
use std::ops::Range;

enum PatchKind {
  Rsync,
  Bsdiff,
}

enum PatchStatus {
  Running(PatchKind),
  Finished,
}

/// Shared state for reading a sequence of patch operations from a stream.
///
/// Wraps a reader and tracks whether the current file's operation stream is
/// still running or has been terminated by its end marker. Used as the backing
/// store for [`RsyncOpIter`] and [`BsdiffOpIter`], which borrow it mutably.
struct PatchOpIter<R: Read> {
  reader: R,
  status: PatchStatus,
}

impl<R: Read> PatchOpIter<R> {
  fn new(reader: R) -> Self {
    Self {
      reader,
      status: PatchStatus::Finished,
    }
  }

  fn drain(&mut self) -> Result<()> {
    match self.status {
      PatchStatus::Finished => Ok(()),
      // The RsyncOpIter and BsdiffOpIter are constructed directly (without calling new)
      // to avoid setting the status again (it has just been checked)
      PatchStatus::Running(PatchKind::Rsync) => RsyncOpIter(self).drain(),
      PatchStatus::Running(PatchKind::Bsdiff) => BsdiffOpIter(self).drain(),
    }
  }
}

pub struct RsyncOpIter<'a, R: Read>(&'a mut PatchOpIter<R>);
pub struct BsdiffOpIter<'a, R: Read>(&'a mut PatchOpIter<R>);

impl<'a, R: Read> RsyncOpIter<'a, R> {
  fn new(patch_iter: &'a mut PatchOpIter<R>) -> Self {
    patch_iter.status = PatchStatus::Running(PatchKind::Rsync);
    Self(patch_iter)
  }

  fn drain(&mut self) -> Result<()> {
    for op in self {
      op?;
    }

    Ok(())
  }
}

impl<'a, R: Read> BsdiffOpIter<'a, R> {
  fn new(patch_iter: &'a mut PatchOpIter<R>) -> Self {
    patch_iter.status = PatchStatus::Running(PatchKind::Bsdiff);
    Self(patch_iter)
  }

  fn drain(&mut self) -> Result<()> {
    for op in self {
      op?;
    }

    Ok(())
  }
}

impl<R: Read> Dump for RsyncOpIter<'_, R> {
  fn dump(&mut self, writer: &mut impl std::io::Write) -> Result<()> {
    for op in self {
      op?.dump(writer)?;
    }

    Ok(())
  }
}

impl<R: Read> Dump for BsdiffOpIter<'_, R> {
  fn dump(&mut self, writer: &mut impl std::io::Write) -> Result<()> {
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
    // To be replaced by std::range::Range<u64> when it is stabilized
    block_range: Range<u64>,
  },
  Data(Box<[u8]>),
}

impl RsyncOp {
  fn from_op(op: SyncOp) -> Result<Self> {
    Ok(match op {
      SyncOp::BlockRange {
        file_index,
        block_index,
        block_span,
      } => {
        // Check that adding the block index and block span doesn't overflow
        let end = block_index.checked_add(block_span).ok_or_else(|| {
          InconsistentMessage::OverflowingBlockSpan {
            block_index,
            block_span,
          }
          .into_error::<SyncOp>()
        })?;

        Self::BlockRange {
          file_index,
          block_range: block_index..end,
        }
      }
      SyncOp::Data(data) => Self::Data(data),
      SyncOp::HeyYouDidIt => unreachable!(),
    })
  }
}

impl Dump for RsyncOp {
  fn dump(&mut self, writer: &mut impl Write) -> Result<()> {
    writeln!(writer, "{:?}", self).map_err(|e| IoError::WriteDumpFailed(e).into())
  }
}

impl<R: Read> Iterator for RsyncOpIter<'_, R> {
  type Item = Result<RsyncOp>;

  /// Decode the next [`SyncOp`] in the stream
  fn next(&mut self) -> Option<Self::Item> {
    if let PatchStatus::Finished = self.0.status {
      return None;
    }

    Some(match SyncOp::decode(&mut self.0.reader) {
      Ok(SyncOp::HeyYouDidIt) => {
        self.0.status = PatchStatus::Finished;
        return None;
      }
      Ok(sync_op) => RsyncOp::from_op(sync_op),
      Err(e) => Err(e),
    })
  }
}

impl<R: Read> FusedIterator for RsyncOpIter<'_, R> {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BsdiffOp {
  pub add: Box<[u8]>,
  pub copy: Box<[u8]>,
  pub seek: i64,
}

impl BsdiffOp {
  fn from_op(op: Control) -> Self {
    match op {
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

impl<R: Read> Iterator for BsdiffOpIter<'_, R> {
  type Item = Result<BsdiffOp>;

  /// Decode the next [`Control`] in the stream
  fn next(&mut self) -> Option<Self::Item> {
    if let PatchStatus::Finished = self.0.status {
      return None;
    }

    Some(match Control::decode(&mut self.0.reader) {
      Ok(Control::Eof) => {
        // After the bsdiff EOF message, a rsync HeyYouDidIt message follows
        match SyncOp::decode(&mut self.0.reader) {
          Ok(SyncOp::HeyYouDidIt) => (),
          Ok(_) => {
            return Some(Err(
              InconsistentMessage::ExpectedHeyYouDidIt.into_error::<SyncOp>(),
            ));
          }
          Err(e) => return Some(Err(e)),
        }

        self.0.status = PatchStatus::Finished;
        return None;
      }
      Ok(sync_op) => Ok(BsdiffOp::from_op(sync_op)),
      Err(e) => Err(e),
    })
  }
}

impl<R: Read> FusedIterator for BsdiffOpIter<'_, R> {}

/// The patch operation kind for a single file, holding the concrete op iterator
///
/// - `Rsync`: the file was patched using rsync block operations and raw data chunks.
/// - `Bsdiff`: the file was patched using bsdiff binary diff controls against
///   the old file at `target_index` in the old container.
pub enum PatchOp<'a, R: Read> {
  Rsync {
    iter: RsyncOpIter<'a, R>,
  },
  Bsdiff {
    iter: BsdiffOpIter<'a, R>,
    #[expect(dead_code)]
    target_index: usize,
  },
}

impl<R: Read> Dump for PatchOp<'_, R> {
  fn dump(&mut self, writer: &mut impl Write) -> Result<()> {
    match self {
      PatchOp::Rsync { iter } => iter.dump(writer),
      PatchOp::Bsdiff { iter, .. } => iter.dump(writer),
    }
  }
}

/// Lending iterator over per-file [`PatchOp`]s in a patch stream
///
/// Yields a `(file_index, `[`PatchOp`]`)` tuple for each file in the new
/// container, in order. The [`PatchOp`] contains all patch operations for
/// that file.
///
/// Before yielding each item, drains any unread operations left over from
/// the previous file.
pub struct FilePatchIter<R: Read> {
  patch_iter: PatchOpIter<R>,

  old_file_sizes: Vec<u64>,
  // To be replaced by std::range::RangeIter<usize> when it is stabilized
  new_file_indexes: Range<usize>,
}

impl<R: Read> FilePatchIter<R> {
  fn new(reader: R, container_old: &Container, container_new: &Container) -> Self {
    // Create the inner patch iter
    let patch_iter = PatchOpIter::new(reader);

    Self {
      patch_iter,
      old_file_sizes: container_old.files.iter().map(|f| f.size).collect(),
      new_file_indexes: 0..container_new.files.len(),
    }
  }

  fn old_files_count(&self) -> usize {
    self.old_file_sizes.len()
  }

  fn check_old_file_index<MessageType>(&self, idx: usize) -> Result<()> {
    if idx < self.old_files_count() {
      Ok(())
    } else {
      Err(
        InconsistentMessage::OutOfBoundsFileIndex {
          container_file_count: self.old_files_count(),
          file_index: idx,
        }
        .into_error::<MessageType>(),
      )
    }
  }
}

impl<R: Read> LendingIterator for FilePatchIter<R> {
  type Item<'a>
    = Result<(usize, PatchOp<'a, R>)>
  where
    R: 'a;

  fn next<'a>(&'a mut self) -> Option<Self::Item<'a>> {
    // Get the next file index or return None if there are no more files to process
    let file_index = self.new_file_indexes.next()?;

    // Skip the patch operations that belong to the last file and have not been read
    if let Err(e) = self.patch_iter.drain() {
      return Some(Err(e));
    }

    // Determine the kind of patch operations for this file
    let header = match SyncHeader::decode(&mut self.patch_iter.reader) {
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

    let patch_op = match header.r#type {
      Type::Rsync => PatchOp::Rsync {
        iter: RsyncOpIter::new(&mut self.patch_iter),
      },
      Type::Bsdiff => match BsdiffHeader::decode(&mut self.patch_iter.reader) {
        Err(e) => return Some(Err(e)),
        Ok(BsdiffHeader { target_index }) => {
          // Check that the target index is in-bounds
          if let Err(e) = self.check_old_file_index::<BsdiffHeader>(target_index) {
            return Some(Err(e));
          }

          PatchOp::Bsdiff {
            iter: BsdiffOpIter::new(&mut self.patch_iter),
            target_index,
          }
        }
      },
    };

    Some(Ok((file_index, patch_op)))
  }
}

impl<R: Read> Dump for FilePatchIter<R> {
  fn dump(&mut self, writer: &mut impl Write) -> Result<()> {
    while let Some(item) = self.next() {
      match item {
        Ok((_file_index, mut patch_iter)) => patch_iter.dump(writer)?,
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
    let patch_iter = FilePatchIter::new(reader, &container_old, &container_new);

    Ok(Patch {
      header,
      container_old,
      container_new,
      patch_iter,
    })
  }
}
