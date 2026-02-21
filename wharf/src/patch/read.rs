use super::{OpIter, Patch, RsyncOp, SyncEntryIter, SyncHeader, SyncHeaderKind, op_kind};
use crate::common::{MAGIC_PATCH, check_magic_bytes, decompress_stream};
use crate::protos::{bsdiff, decode_protobuf, pwr, skip_protobuf, tlc};

use std::io::{BufRead, Read};
use std::marker::PhantomData;

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

    for op in self {
      op?;
    }

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

impl<R> Iterator for OpIter<'_, R, op_kind::Rsync>
where
  R: Read,
{
  type Item = Result<RsyncOp, String>;

  fn next(&mut self) -> Option<Self::Item> {
    if self.finished {
      return None;
    }

    match decode_protobuf::<pwr::SyncOp>(&mut self.reader) {
      Err(e) => Some(Err(format!(
        "Couldn't decode Rsync SyncOp message from reader!\n{e}"
      ))),

      Ok(sync_op) => {
        if sync_op.r#type() == pwr::sync_op::Type::HeyYouDidIt {
          self.finished = true;
          None
        } else {
          Some(Ok(sync_op.into()))
        }
      }
    }
  }
}

impl<R> Iterator for OpIter<'_, R, op_kind::Bsdiff>
where
  R: Read,
{
  type Item = Result<bsdiff::Control, String>;

  fn next(&mut self) -> Option<Self::Item> {
    if self.finished {
      return None;
    }

    match decode_protobuf::<bsdiff::Control>(&mut self.reader) {
      Err(e) => Some(Err(format!(
        "Couldn't decode Bsdiff Control message from reader!\n{e}"
      ))),

      Ok(control_op) => {
        if control_op.eof {
          // Wharf adds a Rsync HeyYouDidIt message after the Bsdiff EOF
          match decode_protobuf::<pwr::SyncOp>(&mut self.reader) {
            Err(e) => Some(Err(format!(
              "Couldn't decode Rsync SyncOp message from reader!\n{e}"
            ))),

            Ok(sync_op) => {
              if sync_op.r#type() == pwr::sync_op::Type::HeyYouDidIt {
                self.finished = true;
                None
              } else {
                Some(Err(
                  "Expected a Rsync HeyYouDidIt sync operation, but did not found it!".to_string(),
                ))
              }
            }
          }
        } else {
          Some(Ok(control_op))
        }
      }
    }
  }
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

  pub fn next_header(&mut self) -> Option<Result<SyncHeader<'_, R>, String>> {
    if self.entries_read == self.total_entries {
      return None;
    }

    self.entries_read += 1;

    // Decode the SyncHeader
    let header = match decode_protobuf::<pwr::SyncHeader>(&mut self.reader) {
      Err(e) => return Some(Err(e)),
      Ok(sync_header) => sync_header,
    };

    // Decode the BsdiffHeader (if the header type is Bsdiff)
    let bsdiff_header = match header.r#type() {
      pwr::sync_header::Type::Rsync => None,
      pwr::sync_header::Type::Bsdiff => {
        match decode_protobuf::<pwr::BsdiffHeader>(&mut self.reader) {
          Err(e) => return Some(Err(e)),
          Ok(bsdiff_header) => Some(bsdiff_header),
        }
      }
    };

    // Pack the gathered data into a SyncHeader struct and return it
    Some(Ok(SyncHeader {
      file_index: header.file_index,
      kind: match bsdiff_header {
        None => SyncHeaderKind::Rsync {
          op_iter: OpIter {
            reader: &mut self.reader,
            finished: false,
            _kind: PhantomData::<op_kind::Rsync>,
          },
        },
        Some(bsdiff) => SyncHeaderKind::Bsdiff {
          target_index: bsdiff.target_index,
          op_iter: OpIter {
            reader: &mut self.reader,
            finished: false,
            _kind: PhantomData::<op_kind::Bsdiff>,
          },
        },
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
      // An entry is provided for each file in the new container
      total_entries: container_new.files.len() as u64,
      entries_read: 0,
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
