use super::{OpStatus, verify_data};
use crate::hasher::BlockHasher;
use crate::protos::bsdiff;

use std::fs;
use std::io::{Read, Seek, Write};

/// Read a block from `src`, add corresponding bytes from `add`, and write the result to `dst`
///
/// After this function is called, the bytes that have been written into the file
/// will be in `add_buffer`
fn add_bytes(
  src: &mut impl Read,
  dst: &mut impl Write,
  add: &[u8],
  add_buffer: &mut [u8],
) -> Result<(), String> {
  assert_eq!(add.len(), add_buffer.len());

  src
    .read_exact(add_buffer)
    .map_err(|e| format!("Couldn't read data from old file into buffer!\n{e}"))?;

  for i in 0..add.len() {
    add_buffer[i] += add[i];
  }

  dst
    .write_all(add_buffer)
    .map_err(|e| format!("Couldn't save buffer data into new file!\n{e}"))
}

impl bsdiff::Control {
  /// Apply the `control` bsdiff operation into the writer
  pub fn apply(
    &self,
    writer: &mut impl Write,
    hasher: &mut Option<BlockHasher<'_, impl Read>>,
    old_file: &mut fs::File,
    add_buffer: &mut Vec<u8>,
    progress_callback: &mut impl FnMut(u64),
  ) -> Result<OpStatus, String> {
    // Control operations must be applied in order
    // First, add the diff bytes
    if !self.add.is_empty() {
      // Resize the add buffer to match the size of the current add bytes
      // The add operations are usually the same length, so allocation is almost never triggered
      // If the new add bytes are smaller than the buffer size, allocation will also be avoided
      add_buffer.resize(self.add.len(), 0);

      add_bytes(old_file, writer, &self.add, add_buffer)?;

      // Verify the written data
      if let OpStatus::VerificationFailed = verify_data(hasher, add_buffer)? {
        return Ok(OpStatus::VerificationFailed);
      }

      // Return the number of bytes added into the new file
      progress_callback(self.add.len() as u64);
    }

    // Then, copy the extra bytes
    if !self.copy.is_empty() {
      writer
        .write_all(&self.copy)
        .map_err(|e| format!("Couldn't copy data from patch to new file!\n{e}"))?;

      // Verify the written data
      if let OpStatus::VerificationFailed = verify_data(hasher, &self.copy)? {
        return Ok(OpStatus::VerificationFailed);
      }

      // Return the number of bytes copied into the new file
      progress_callback(self.copy.len() as u64);
    }

    // Lastly, seek into the correct position in the old file
    if self.seek != 0 {
      old_file.seek_relative(self.seek).map_err(|e| {
        format!(
          "Couldn't seek into old file at relative pos: {}\n{e}",
          self.seek
        )
      })?;
    }

    Ok(OpStatus::Ok)
  }
}
