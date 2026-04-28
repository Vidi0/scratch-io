//! Magic numbers and binary header utilities for wharf file formats.
//!
//! Wharf binary files begin with a 4-byte little-endian magic number that identifies
//! the file format. This module provides:
//!
//! - The magic numbers for each wharf file format (.pwr, .pws, .pwm, .pww, .pzi)
//! - [`read_magic_bytes`] to read the magic number from a reader
//! - [`check_magic_bytes`] to verify it matches an expected value

use crate::binaries::read_wharf_exact;
use crate::errors::{InvalidWharfBinary, Result};

use std::io::Read;

/// Magic number for wharf patch files (.pwr)
///
/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L14>
pub const PATCH_MAGIC: u32 = 0x0FEF_5F00;

/// Magic number for wharf signature files (.pws)
///
/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L17>
pub const SIGNATURE_MAGIC: u32 = PATCH_MAGIC + 1;

/// Magic number for wharf manifest files (.pwm)
///
/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L20>
#[expect(dead_code)]
pub const MANIFEST_MAGIC: u32 = PATCH_MAGIC + 2;

/// Magic number for wharf wounds file (.pww)
///
/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L23>
#[expect(dead_code)]
pub const WOUNDS_MAGIC: u32 = PATCH_MAGIC + 3;

/// Magic number for wharf zip index files (.pzi)
///
/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L26>
#[expect(dead_code)]
pub const ZIP_INDEX_MAGIC: u32 = PATCH_MAGIC + 4;

/// Read the magic bytes of the provided reader as an `u32`
pub fn read_magic_bytes(reader: &mut impl Read) -> Result<u32> {
  // Read the next 4 bytes of the reader
  let mut magic_bytes = [0u8; 4];
  read_wharf_exact(reader, &mut magic_bytes)?;

  // Return the little endian u32 representation of the bytes
  Ok(u32::from_le_bytes(magic_bytes))
}

/// Verify that the magic bytes of the reader match the expected magic number
///
/// # Errors
///
/// If the bytes couldn't be read from the reader or the magic bytes don't match
pub fn check_magic_bytes(reader: &mut impl Read, expected_magic: u32) -> Result<()> {
  let magic = read_magic_bytes(reader)?;

  // Compare the magic numbers
  if magic == expected_magic {
    Ok(())
  } else {
    Err(
      InvalidWharfBinary::MagicMismatch {
        expected: expected_magic,
        found: magic,
      }
      .into(),
    )
  }
}
