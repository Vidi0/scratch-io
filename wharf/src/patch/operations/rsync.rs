use super::{FilesCache, FilesCacheStatus, OpStatus, verify_data};
use crate::common::BLOCK_SIZE;
use crate::hasher::{BlockHasherStatus, FileBlockHasher};
use crate::patch::RsyncOp;
use crate::protos::tlc;

use std::io::{self, Read, Seek, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
enum CopyRangeStatus {
  Ok(u64),
  Broken,
}

/// Copy blocks of bytes from `src` into `dst`
#[allow(clippy::too_many_arguments)]
fn copy_range(
  src: &mut (impl Read + Seek),
  dst: &mut impl Write,
  hasher: &mut Option<FileBlockHasher<impl Read>>,
  block_index: u64,
  block_span: u64,
  old_file_container_size: u64,
  old_file_disk_size: u64,
  buffer: &mut [u8],
) -> Result<CopyRangeStatus, String> {
  let start_pos = block_index * BLOCK_SIZE;
  let len = {
    // The patch operation will copy this number of bytes:
    // the minimum between the range specified and the remaining number
    // of bytes in the container file.
    let remaining_file_bytes = old_file_container_size - start_pos;
    let bytes_to_copy = (block_span * BLOCK_SIZE).min(remaining_file_bytes);

    // If the file in disk doesn't have enought bytes, set
    // the file as broken (we won't be able to patch it).
    if start_pos + bytes_to_copy > old_file_disk_size {
      return Ok(CopyRangeStatus::Broken);
    }

    bytes_to_copy
  };

  src
    .seek(io::SeekFrom::Start(start_pos))
    .map_err(|e| format!("Couldn't seek into old file at pos: {start_pos}\n{e}"))?;

  let mut limited = src.take(len);

  // Read the data, write it, and then hash it
  let mut total_written: u64 = 0;

  // Check the buffer has been resized correctly
  assert_ne!(buffer.len(), 0);

  loop {
    // Read the data into the buffer
    let read = limited
      .read(buffer)
      .map_err(|e| format!("Couldn't read from old file\n{e}"))?;

    if read == 0 {
      break;
    }

    // Write the data into the new file
    dst
      .write_all(&buffer[..read])
      .map_err(|e| format!("Couldn't write to new file\n{e}"))?;

    // Update the hasher
    if let Some(hasher) = hasher {
      let status = hasher.update(&buffer[..read])?;
      if let BlockHasherStatus::HashMismatch { .. } = status {
        return Ok(CopyRangeStatus::Broken);
      }
    }

    total_written += read as u64;
  }

  Ok(CopyRangeStatus::Ok(total_written))
}

impl RsyncOp {
  /// Check if this `RsyncOp` represents a file copy from the
  /// old container into the new without changing the data
  ///
  /// Returns an option containing the old file index
  pub fn is_literal_copy(
    &self,
    new_file_size: u64,
    container_old: &tlc::Container,
  ) -> Result<Option<usize>, String> {
    // The type must be BlockRange
    if let Self::BlockRange {
      file_index,
      block_index,
      block_span,
    } = *self
    {
      Ok(
        // It should copy from the first block until the end of the given file
        (block_index == 0
          && block_span * BLOCK_SIZE >= new_file_size
        // The size of the old and the new file must be equal
          && new_file_size == container_old.get_file(file_index)?.size as u64)
          .then_some(file_index),
      )
    } else {
      Ok(None)
    }
  }

  /// Check if this `SyncOp` represents an empty file
  pub fn is_empty_file(&self, new_file_size: u64) -> bool {
    // The type must be Data
    if let Self::Data(data) = self {
      // The data field should be empty
      data.is_empty()
      // The new file must have a 0 size in the container
        && new_file_size == 0
    } else {
      false
    }
  }

  /// Apply the `op` rsync operation into the writer
  pub fn apply(
    &self,
    writer: &mut impl Write,
    hasher: &mut Option<FileBlockHasher<impl Read>>,
    old_files_cache: &mut FilesCache,
    container_old: &tlc::Container,
    buffer: &mut [u8],
  ) -> Result<OpStatus, String> {
    let mut written_bytes: u64 = 0;

    match *self {
      // If the type is BlockRange, copy the range from the old file to the new one
      Self::BlockRange {
        file_index,
        block_index,
        block_span,
      } => {
        // Open the old file
        let (old_file, old_file_container_size, old_file_disk_size) =
          match old_files_cache.get_file(file_index, container_old)? {
            FilesCacheStatus::Ok {
              file,
              container_size,
              disk_size,
            } => (file, container_size, disk_size),
            FilesCacheStatus::NotFound => return Ok(OpStatus::Broken),
          };

        // Rewind isn't needed because the copy_range function already seeks
        // into the correct (not relative) position

        // Copy the specified range to the new file
        let status = copy_range(
          old_file,
          writer,
          hasher,
          block_index,
          block_span,
          old_file_container_size,
          old_file_disk_size,
          buffer,
        )?;

        // The copy_range function already verified the data

        // Return the number of bytes copied into the new file or the error
        match status {
          CopyRangeStatus::Ok(b) => written_bytes += b,
          CopyRangeStatus::Broken => return Ok(OpStatus::Broken),
        }
      }
      // If the type is Data, just copy the data from the patch to the new file
      Self::Data(ref data) => {
        writer
          .write_all(data)
          .map_err(|e| format!("Couldn't copy data from patch to new file!\n{e}"))?;

        // Verify the written data
        match verify_data(hasher, data)? {
          OpStatus::Ok { written_bytes: b } => written_bytes += b,
          OpStatus::Broken => return Ok(OpStatus::Broken),
        }
      }
    }

    Ok(OpStatus::Ok { written_bytes })
  }
}
