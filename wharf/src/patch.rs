pub mod apply;
pub mod operations;

use crate::common::{MAGIC_PATCH, check_magic_bytes, decompress_stream};
use crate::protos::{bsdiff, pwr, tlc};
use crate::protos::{decode_protobuf, skip_protobuf};

use std::io::{BufRead, Read};
use std::marker::PhantomData;

pub mod op_kind {
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub struct Rsync;
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub struct Bsdiff;
}

#[derive(Debug, PartialEq, Eq)]
pub struct OpIter<'a, R, K> {
  reader: &'a mut R,
  finished: bool,
  _kind: PhantomData<K>,
}

impl<R, K, T> OpIter<'_, R, K>
where
  Self: Iterator<Item = Result<T, String>>,
{
  /// Drain the op iterator
  ///
  /// It is very important to drain the iterator before getting
  /// the next one if it hasn't been fully consumed.
  /// If the iterator isn't drained, the next [`SyncEntryIter::next_header`]
  /// call will fail because of an invalid read offset.
  pub fn drain(&mut self) -> Result<(), String> {
    if self.finished {
      return Ok(());
    }

    for op in &mut *self {
      op?;
    }

    self.finished = true;

    Ok(())
  }
}

impl<R, K> OpIter<'_, R, K>
where
  R: Read,
{
  pub fn skip_operations(&mut self, operations_to_skip: u64) -> Result<(), String> {
    for _ in 0..operations_to_skip {
      skip_protobuf(&mut self.reader)?;
    }

    Ok(())
  }
}

impl<R, K, T> OpIter<'_, R, K>
where
  Self: Iterator<Item = Result<T, String>>,
  T: std::fmt::Debug,
{
  pub fn dump_stdout(&mut self) -> Result<(), String> {
    for op in self {
      println!("{:?}", op?);
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
  Data(Vec<u8>),
}

impl From<pwr::SyncOp> for RsyncOp {
  fn from(value: pwr::SyncOp) -> Self {
    use pwr::sync_op::Type as T;

    match value.r#type() {
      T::HeyYouDidIt => unreachable!(),
      T::BlockRange => Self::BlockRange {
        file_index: value.file_index as usize,
        block_index: value.block_index as u64,
        block_span: value.block_span as u64,
      },
      T::Data => Self::Data(value.data),
    }
  }
}

impl<R> Iterator for OpIter<'_, R, op_kind::Rsync>
where
  R: Read,
{
  type Item = Result<RsyncOp, String>;

  fn next(&mut self) -> Option<Self::Item> {
    if self.finished {
      return None;
    }

    let sync_op = match decode_protobuf::<pwr::SyncOp>(&mut self.reader) {
      Ok(sync_op) => sync_op,
      Err(e) => {
        return Some(Err(format!(
          "Couldn't decode Rsync SyncOp message from reader!\n{e}"
        )));
      }
    };

    if sync_op.r#type() == pwr::sync_op::Type::HeyYouDidIt {
      self.finished = true;
      None
    } else {
      Some(Ok(sync_op.into()))
    }
  }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BsdiffOp {
  pub add: Vec<u8>,
  pub copy: Vec<u8>,
  pub seek: i64,
}

impl From<bsdiff::Control> for BsdiffOp {
  fn from(value: bsdiff::Control) -> Self {
    Self {
      add: value.add,
      copy: value.copy,
      seek: value.seek,
    }
  }
}

impl<R> Iterator for OpIter<'_, R, op_kind::Bsdiff>
where
  R: Read,
{
  type Item = Result<BsdiffOp, String>;

  fn next(&mut self) -> Option<Self::Item> {
    if self.finished {
      return None;
    }

    let control_op = match decode_protobuf::<bsdiff::Control>(&mut self.reader) {
      Ok(control_op) => control_op,
      Err(e) => {
        return Some(Err(format!(
          "Couldn't decode Bsdiff Control message from reader!\n{e}"
        )));
      }
    };

    if control_op.eof {
      // Wharf adds a Rsync HeyYouDidIt message after the Bsdiff EOF
      let sync_op = match decode_protobuf::<pwr::SyncOp>(&mut self.reader) {
        Ok(sync_op) => sync_op,
        Err(e) => {
          return Some(Err(format!(
            "Couldn't decode Rsync SyncOp message from reader!\n{e}"
          )));
        }
      };

      if sync_op.r#type() == pwr::sync_op::Type::HeyYouDidIt {
        self.finished = true;
        None
      } else {
        Some(Err(
          "Expected a Rsync HeyYouDidIt sync operation, but did not found it!".to_string(),
        ))
      }
    } else {
      Some(Ok(control_op.into()))
    }
  }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SyncHeaderKind<'a, R> {
  Rsync {
    op_iter: OpIter<'a, R, op_kind::Rsync>,
  },
  Bsdiff {
    target_index: usize,
    op_iter: OpIter<'a, R, op_kind::Bsdiff>,
  },
}

#[derive(Debug, PartialEq, Eq)]
pub struct SyncHeader<'a, R> {
  pub file_index: usize,
  pub kind: SyncHeaderKind<'a, R>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncEntryIter<R> {
  reader: R,
  remaining_entries: u64,
}

impl<R> SyncEntryIter<R>
where
  R: Read,
{
  pub fn dump_stdout(&mut self) -> Result<(), String> {
    while let Some(header) = self.next_header() {
      let header = header?;

      // Print the new file index
      println!("\n{}", header.file_index);

      // Print all the patch operations
      match header.kind {
        SyncHeaderKind::Rsync { mut op_iter } => {
          println!("Rsync");
          op_iter.dump_stdout()?;
        }
        SyncHeaderKind::Bsdiff {
          target_index,
          mut op_iter,
        } => {
          println!("Bsdiff: {target_index}");
          op_iter.dump_stdout()?;
        }
      }
    }

    Ok(())
  }

  fn new_op_iter<K>(&mut self) -> OpIter<'_, R, K> {
    OpIter {
      reader: &mut self.reader,
      finished: false,
      _kind: PhantomData,
    }
  }

  pub fn next_header(&mut self) -> Option<Result<SyncHeader<'_, R>, String>> {
    if self.remaining_entries == 0 {
      return None;
    }

    self.remaining_entries -= 1;

    // Decode the SyncHeader
    let header = match decode_protobuf::<pwr::SyncHeader>(&mut self.reader) {
      Ok(sync_header) => sync_header,
      Err(e) => return Some(Err(e)),
    };

    // Pack the gathered data into a SyncHeader struct and return it
    use pwr::sync_header::Type;
    Some(Ok(SyncHeader {
      file_index: header.file_index as usize,
      kind: match header.r#type() {
        Type::Rsync => SyncHeaderKind::Rsync {
          op_iter: self.new_op_iter(),
        },
        Type::Bsdiff => {
          // If the header type is Bsdiff, decode the BsdiffHeader
          let target_index = match decode_protobuf::<pwr::BsdiffHeader>(&mut self.reader) {
            Ok(bsdiff_header) => bsdiff_header.target_index as usize,
            Err(e) => return Some(Err(e)),
          };

          SyncHeaderKind::Bsdiff {
            target_index,
            op_iter: self.new_op_iter(),
          }
        }
      },
    }))
  }

  pub fn skip_entries(&mut self, entries_to_skip: u64) -> Result<(), String> {
    // For each entry that will be skipped:
    for i in 0..entries_to_skip {
      // Get the header
      let header = match self.next_header() {
        Some(Ok(h)) => h,
        Some(Err(e)) => return Err(format!("Couldn't get next patch sync operation!\n{e}")),
        None => {
          return Err(format!(
            "Can't skip {} entries, the iter stops at {}!",
            entries_to_skip, i
          ));
        }
      };

      // Drain the corresponding iterator
      match header.kind {
        SyncHeaderKind::Rsync { mut op_iter } => {
          op_iter.drain()?;
        }
        SyncHeaderKind::Bsdiff { mut op_iter, .. } => {
          op_iter.drain()?;
        }
      }
    }

    Ok(())
  }
}

/// Represents a decoded wharf patch file
///
/// <https://docs.itch.zone/wharf/master/file-formats/patches.html>
///
/// Contains the header, the old and new containers describing file system
/// state before and after the patch, and an iterator over the patch operations.
/// The iterator reads from the underlying stream on the fly as items are requested.
pub struct Patch<'a> {
  pub header: pwr::PatchHeader,
  pub container_old: tlc::Container,
  pub container_new: tlc::Container,
  pub sync_op_iter: SyncEntryIter<Box<dyn BufRead + 'a>>,
}

impl<'a> Patch<'a> {
  /// Dump the patch contents to standard output
  ///
  /// This prints the header, container metadata, and all patch operations
  /// for inspection by a human reader. The internal patch iterator is
  /// consumed during this call.
  pub fn dump_stdout(&mut self) -> Result<(), String> {
    // Print the header
    println!("{:?}", self.header);

    // Print the old container
    println!("\n--- START OLD CONTAINER INFO ---\n");
    self.container_old.dump_stdout();
    println!("\n--- END OLD CONTAINER INFO ---");

    // Print the new container
    println!("--- START NEW CONTAINER INFO ---\n");
    self.container_new.dump_stdout();
    println!("\n--- END NEW CONTAINER INFO ---");

    // Print the patch operations
    println!("--- START PATCH OPERATIONS ---");
    self.sync_op_iter.dump_stdout()?;
    println!("\n--- END PATCH OPERATIONS ---");

    Ok(())
  }

  /// Print a concise summary of the patch to standard output
  ///
  /// Shows the compression settings and basic statistics of the
  /// old and new containers (size, number of files, directories, and symlinks).
  pub fn print_summary(&self) {
    // Print the kind of binary
    println!(
      "wharf patch file ({})",
      // If the Patch was read using Patch::read or Patch::read_without_magic,
      // then the compression field MUST be Some, because otherwise reading would have failed
      self.header.compression.unwrap()
    );

    // Print the old container stats
    self.container_old.print_summary("old");

    // Print the new container stats
    self.container_new.print_summary("new");
  }

  /// Decode a binary wharf patch assuming the magic bytes
  /// have already been consumed from the input stream
  ///
  /// For more information, see [`Patch::read`].
  pub fn read_without_magic(reader: &'a mut impl BufRead) -> Result<Self, String> {
    // Decode the patch header
    let header = decode_protobuf::<pwr::PatchHeader>(reader)?;

    // Decompress the remaining stream
    let compression_algorithm = header
      .compression
      .ok_or("Missing compressing field in Patch Header!")?
      .algorithm();

    let mut decompressed = decompress_stream(reader, compression_algorithm)?;

    // Decode the containers
    let container_old = decode_protobuf::<tlc::Container>(&mut decompressed)?;
    let container_new = decode_protobuf::<tlc::Container>(&mut decompressed)?;

    // Decode the sync operations
    let sync_op_iter = SyncEntryIter {
      reader: decompressed,
      remaining_entries: container_new.files.len() as u64,
    };

    Ok(Patch {
      header,
      container_old,
      container_new,
      sync_op_iter,
    })
  }

  /// Decode a binary wharf patch
  ///
  /// If the magic bytes have already been read, use [`Patch::read_without_magic`].
  ///
  /// # References
  ///
  /// <https://docs.itch.zone/wharf/master/file-formats/signatures.html>
  ///
  /// <https://github.com/Vidi0/scratch-io/blob/main/docs/wharf/patch.md>
  pub fn read(reader: &'a mut impl BufRead) -> Result<Self, String> {
    // Check the magic bytes
    check_magic_bytes(reader, MAGIC_PATCH)?;

    // Decode the remaining data
    Self::read_without_magic(reader)
  }
}
