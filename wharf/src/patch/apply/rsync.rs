use super::{OpStatus, verify_data};
use crate::common::BLOCK_SIZE;
use crate::hasher::BlockHasher;
use crate::protos::{pwr, tlc};

use std::fs;
use std::io::{self, Read, Seek, Write};
use std::path::Path;

/// Copy blocks of bytes from `src` into `dst`
fn copy_range(
  src: &mut (impl Read + Seek),
  dst: &mut impl Write,
  //////////// TODO
  //
  // Make this hasher actually hash something
  _hasher: &mut Option<BlockHasher<'_, impl Read>>,
  block_index: u64,
  block_span: u64,
) -> Result<u64, String> {
  let start_pos = block_index * BLOCK_SIZE;
  let len = block_span * BLOCK_SIZE;

  src
    .seek(io::SeekFrom::Start(start_pos))
    .map_err(|e| format!("Couldn't seek into old file at pos: {start_pos}\n{e}"))?;

  let mut limited = src.take(len);

  io::copy(&mut limited, dst).map_err(|e| format!("Couldn't copy data from old file to new!\n{e}"))
}

impl pwr::SyncOp {
  /// Apply the `op` rsync operation into the writer
  pub fn apply(
    &self,
    writer: &mut impl Write,
    hasher: &mut Option<BlockHasher<'_, impl Read>>,
    old_files_cache: &mut lru::LruCache<usize, fs::File>,
    container_old: &tlc::Container,
    old_build_folder: &Path,
    progress_callback: &mut impl FnMut(u64),
  ) -> Result<OpStatus, String> {
    match self.r#type() {
      // If the type is BlockRange, copy the range from the old file to the new one
      pwr::sync_op::Type::BlockRange => {
        // Open the old file
        let old_file = old_files_cache.try_get_or_insert_mut(self.file_index as usize, || {
          container_old.open_file_read(self.file_index as usize, old_build_folder.to_owned())
        })?;

        // Rewind isn't needed because the copy_range function already seeks
        // into the correct (not relative) position

        // Copy the specified range to the new file
        let written_bytes = copy_range(
          old_file,
          writer,
          hasher,
          self.block_index as u64,
          self.block_span as u64,
        )?;

        // The copy_range function already verified the data

        // Return the number of bytes copied into the new file
        progress_callback(written_bytes)
      }
      // If the type is Data, just copy the data from the patch to the new file
      pwr::sync_op::Type::Data => {
        writer
          .write_all(&self.data)
          .map_err(|e| format!("Couldn't copy data from patch to new file!\n{e}"))?;

        // Verify the written data
        if let OpStatus::VerificationFailed = verify_data(hasher, &self.data)? {
          return Ok(OpStatus::VerificationFailed);
        }

        // Return the number of bytes written into the new file
        progress_callback(self.data.len() as u64)
      }
      // If the type is HeyYouDidIt, then the iterator would have returned None
      pwr::sync_op::Type::HeyYouDidIt => unreachable!(),
    }

    Ok(OpStatus::Ok)
  }

  /// Check if this `SyncOp` represents a file copy from the
  /// old container into the new without changing the data
  pub fn is_literal_copy(
    &self,
    new_file_size: u64,
    container_old: &tlc::Container,
  ) -> Result<bool, String> {
    Ok(
      // The type must be BlockRange
      self.r#type() == pwr::sync_op::Type::BlockRange
      // It should copy from the first block until the end of the given file
        && self.block_index == 0
        && self.block_span as u64 * BLOCK_SIZE >= new_file_size
      // The size of the old and the new file must be equal
        && new_file_size == container_old.get_file(self.file_index as usize)?.size as u64,
    )
  }
}
