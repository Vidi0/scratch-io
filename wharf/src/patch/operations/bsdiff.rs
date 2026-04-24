use super::OpStatus;
use crate::patch::BsdiffOp;

use std::io::{Read, Seek, Write};

/// Add each byte of `src` to the corresponding byte of `dst` in place, wrapping on overflow.
///
/// Panics if `src` and `dst` have different lengths.
fn add_slices(src: &[u8], dst: &mut [u8]) {
  assert_eq!(src.len(), dst.len());

  for (s, d) in src.iter().zip(dst) {
    *d = d.wrapping_add(*s);
  }
}

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
  src
    .read_exact(add_buffer)
    .map_err(|e| format!("Couldn't read data from old file into buffer!\n{e}"))?;

  add_slices(add, add_buffer);

  dst
    .write_all(add_buffer)
    .map_err(|e| format!("Couldn't save buffer data into new file!\n{e}"))
}

impl BsdiffOp {
  /// Apply the `control` bsdiff operation into the writer
  pub fn apply(
    &self,
    writer: &mut impl Write,
    old_file: &mut (impl Read + Seek),
    old_file_seek_position: &mut u64,
    old_file_disk_size: u64,
    add_buffer: &mut Vec<u8>,
  ) -> Result<OpStatus, String> {
    let mut written_bytes: u64 = 0;

    // Control operations must be applied in order
    // First, add the diff bytes
    if !self.add.is_empty() {
      // If there isn't enough data remaining in this file in the
      // disk to complete the patch operation, set this file as broken
      // (we won't be able to patch it)
      if *old_file_seek_position + self.add.len() as u64 > old_file_disk_size {
        return Ok(OpStatus::Broken);
      }

      // Resize the add buffer if it's smaller than the current add bytes
      // The add operations are usually the same length, so allocation is almost never triggered
      // If the buffer is larger than needed, the excess is ignored via slice truncation
      if add_buffer.len() < self.add.len() {
        add_buffer.resize(self.add.len(), 0);
      }

      let add_buf = &mut add_buffer[..self.add.len()];

      add_bytes(old_file, writer, &self.add, add_buf)?;

      // Move the old file seek cursor forward!
      *old_file_seek_position += self.add.len() as u64;
      written_bytes += self.add.len() as u64;
    }

    // Then, copy the extra bytes
    if !self.copy.is_empty() {
      writer
        .write_all(&self.copy)
        .map_err(|e| format!("Couldn't copy data from patch to new file!\n{e}"))?;

      written_bytes += self.copy.len() as u64;
    }

    // Lastly, seek into the correct position in the old file
    if self.seek != 0 {
      // Add the relative seek into the absolute seek
      if let Some(seek) = old_file_seek_position.checked_add_signed(self.seek) {
        *old_file_seek_position = seek;
      } else {
        return Err(
          "The patch file contains an invalid seek position that causes an overflow!".to_string(),
        );
      };

      // Checking if the new seek position is valid isn't necessary because the OS won't error
      // on out-of-bounds seeks, and the subsequent read (in the add operation) will catch it

      old_file
        .seek(std::io::SeekFrom::Start(*old_file_seek_position))
        .map_err(|e| {
          format!(
            "Couldn't seek into old file at the absolute position: {}\n{e}",
            *old_file_seek_position
          )
        })?;
    }

    Ok(OpStatus::Ok { written_bytes })
  }
}

#[cfg(test)]
mod tests {
  use super::add_bytes;

  #[test]
  fn add_zero() {
    const TEST_DATA_LENGTH: usize = 8;

    let source_bytes: [u8; TEST_DATA_LENGTH] = [0, 87, 124, 143, 49, 81, 215, 248];
    let bytes_to_add: [u8; TEST_DATA_LENGTH] = [0u8; TEST_DATA_LENGTH];

    let mut add_buffer: [u8; TEST_DATA_LENGTH] = [0; TEST_DATA_LENGTH];
    let mut dst: [u8; TEST_DATA_LENGTH] = [0; TEST_DATA_LENGTH];

    add_bytes(
      &mut source_bytes.as_slice(),
      &mut &mut dst.as_mut_slice(),
      &bytes_to_add,
      &mut add_buffer,
    )
    .unwrap();

    assert_eq!(dst, add_buffer);
    // Adding zeros should not change the original bytes
    assert_eq!(add_buffer, source_bytes);
  }

  #[test]
  fn add_wrapping() {
    const TEST_DATA_LENGTH: usize = 4;

    let source_bytes: [u8; TEST_DATA_LENGTH] = [1, 200, 255, 255];
    let bytes_to_add: [u8; TEST_DATA_LENGTH] = [255, 200, 100, 255];
    let expected_result: [u8; TEST_DATA_LENGTH] = [
      0,   // (1 + 255) mod 256 = 256 mod 256 = 0
      144, // (200 + 200) mod 256 = 400 mod 256 = 144
      99,  // (255 + 100) mod 256 = 355 mod 256 = 99
      254, // (255 + 255) mod 256 = 510 mod 256 = 254
    ];

    let mut add_buffer: [u8; TEST_DATA_LENGTH] = [0; TEST_DATA_LENGTH];
    let mut dst: [u8; TEST_DATA_LENGTH] = [0; TEST_DATA_LENGTH];

    add_bytes(
      &mut source_bytes.as_slice(),
      &mut dst.as_mut_slice(),
      &bytes_to_add,
      &mut add_buffer,
    )
    .unwrap();

    assert_eq!(dst, add_buffer);
    assert_eq!(add_buffer, expected_result);
  }

  #[test]
  fn add_single_byte() {
    const TEST_DATA_LENGTH: usize = 1;

    let source_bytes: [u8; TEST_DATA_LENGTH] = [37];
    let bytes_to_add: [u8; TEST_DATA_LENGTH] = [67];
    let expected_result: [u8; TEST_DATA_LENGTH] = [
      104, // 37 + 67 = 104
    ];

    let mut add_buffer: [u8; TEST_DATA_LENGTH] = [0; TEST_DATA_LENGTH];
    let mut dst: [u8; TEST_DATA_LENGTH] = [0; TEST_DATA_LENGTH];

    add_bytes(
      &mut source_bytes.as_slice(),
      &mut dst.as_mut_slice(),
      &bytes_to_add,
      &mut add_buffer,
    )
    .unwrap();

    assert_eq!(dst, add_buffer);
    assert_eq!(add_buffer, expected_result);
  }

  #[test]
  fn add_large_buffer() {
    const TEST_DATA_LENGTH: usize = 10000;

    let source_bytes = vec![48u8; TEST_DATA_LENGTH];
    let bytes_to_add = vec![153u8; TEST_DATA_LENGTH];
    let expected_result = vec![201u8; TEST_DATA_LENGTH]; // 48 + 153 = 201

    let mut add_buffer = vec![0u8; TEST_DATA_LENGTH];
    let mut dst = vec![0u8; TEST_DATA_LENGTH];

    add_bytes(
      &mut source_bytes.as_slice(),
      &mut dst.as_mut_slice(),
      &bytes_to_add,
      &mut add_buffer,
    )
    .unwrap();

    assert_eq!(dst, add_buffer);
    assert_eq!(add_buffer, expected_result);
  }
}
