use crate::patch::BsdiffOpIter;

use std::fs;
use std::io::{Read, Seek, Write};

/// Read a block from `src`, add corresponding bytes from `add`, and write the result to `dst`
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

/// Apply all `op_iter` bsdiff operations to regenerate the new file
/// into `writer` from `old_file`
pub fn apply_bsdiff(
  op_iter: &mut BsdiffOpIter<impl Read>,
  writer: &mut impl Write,
  old_file: &mut fs::File,
  add_buffer: &mut Vec<u8>,
  progress_callback: &mut impl FnMut(u64),
) -> Result<(), String> {
  // Apply all the control operations
  for control in op_iter {
    let control = control?;

    // Control operations must be applied in order
    // First, add the diff bytes
    if !control.add.is_empty() {
      // Resize the add buffer to match the size of the current add bytes
      // The add operations are usually the same length, so allocation is almost never triggered
      // If the new add bytes are smaller than the buffer size, allocation will also be avoided
      add_buffer.resize(control.add.len(), 0);

      add_bytes(old_file, writer, &control.add, add_buffer)?;

      // Return the number of bytes added into the new file
      progress_callback(control.add.len() as u64);
    }

    // Then, copy the extra bytes
    if !control.copy.is_empty() {
      writer
        .write_all(&control.copy)
        .map_err(|e| format!("Couldn't copy data from patch to new file!\n{e}"))?;

      // Return the number of bytes copied into the new file
      progress_callback(control.copy.len() as u64);
    }

    // Lastly, seek into the correct position in the old file
    if control.seek != 0 {
      old_file.seek_relative(control.seek).map_err(|e| {
        format!(
          "Couldn't seek into old file at relative pos: {}\n{e}",
          control.seek
        )
      })?;
    }
  }

  Ok(())
}
