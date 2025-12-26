use super::common::{PATCH_MAGIC, check_magic_bytes, decompress_stream};
use super::protos::*;

use std::io::BufRead;

/// Represents a decoded wharf patch file
///
/// <https://docs.itch.zone/wharf/master/file-formats/patches.html>
///
/// Contains the header, the old and new containers describing file system
/// state before and after the patch, and an iterator over the patch operations.
/// The iterator reads from the underlying stream on the fly as items are requested.
#[derive(Debug, Clone, PartialEq)]
pub struct Patch<R> {
  pub header: pwr::PatchHeader,
  pub container_old: tlc::Container,
  pub container_new: tlc::Container,
  pub sync_op_iter: SyncEntryIter<R>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RsyncOpIter<'a, R> {
  reader: &'a mut R,
}

impl<R> Iterator for RsyncOpIter<'_, R>
where
  R: BufRead,
{
  type Item = Result<pwr::SyncOp, String>;

  fn next(&mut self) -> Option<Self::Item> {
    match decode_protobuf::<pwr::SyncOp>(&mut self.reader) {
      Err(e) => Some(Err(format!(
        "Couldn't decode Rsync SyncOp message from reader!\n{e}"
      ))),

      Ok(sync_op) => {
        if sync_op.r#type() == pwr::sync_op::Type::HeyYouDidIt {
          None
        } else {
          Some(Ok(sync_op))
        }
      }
    }
  }
}

#[derive(Debug, PartialEq, Eq)]
pub struct BsdiffOpIter<'a, R> {
  reader: &'a mut R,
}

impl<R> Iterator for BsdiffOpIter<'_, R>
where
  R: BufRead,
{
  type Item = Result<bsdiff::Control, String>;

  fn next(&mut self) -> Option<Self::Item> {
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

#[derive(Debug, PartialEq, Eq)]
pub enum SyncHeader<'a, R> {
  Rsync {
    file_index: i64,
    op_iter: RsyncOpIter<'a, R>,
  },
  Bsdiff {
    file_index: i64,
    target_index: i64,
    op_iter: BsdiffOpIter<'a, R>,
  },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncEntryIter<R> {
  reader: R,
}

impl<'a, R> SyncEntryIter<R>
where
  R: BufRead,
{
  pub fn next_header(&'a mut self) -> Option<Result<SyncHeader<'a, R>, String>> {
    match self.reader.fill_buf() {
      // If it couldn't read from the stream, return an error
      Err(e) => Some(Err(format!("Couldn't read from reader into buffer!\n{e}"))),

      // If there isn't any data remaining, return None
      Ok([]) => None,

      // If there is data remaining, return the decoded header
      Ok(_) => {
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
        Some(Ok(match bsdiff_header {
          None => SyncHeader::Rsync {
            file_index: header.file_index,
            op_iter: RsyncOpIter {
              reader: &mut self.reader,
            },
          },
          Some(bsdiff) => SyncHeader::Bsdiff {
            file_index: header.file_index,
            target_index: bsdiff.target_index,
            op_iter: BsdiffOpIter {
              reader: &mut self.reader,
            },
          },
        }))
      }
    }
  }
}

/// <https://docs.itch.zone/wharf/master/file-formats/patches.html>
///
/// <https://github.com/Vidi0/scratch-io/blob/main/docs/wharf/patch.md>
pub fn read_patch(reader: &mut impl BufRead) -> Result<Patch<impl BufRead>, String> {
  // Check the magic bytes
  check_magic_bytes(reader, PATCH_MAGIC)?;

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
  };

  Ok(Patch {
    header,
    container_old,
    container_new,
    sync_op_iter,
  })
}
