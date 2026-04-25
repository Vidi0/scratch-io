use crate::errors::{InvalidWharfBinary, IoError, Result};
use crate::magic::check_magic_bytes;

use std::io::{self, Read};

/// Reads the exact number of bytes required to fill `buf`.
///
/// Maps the error to return an [`InvalidWharfBinary::UnexpectedEOF`] error
/// if an unexpected EOF was encountered, because calling this function means
/// data was expected from the wharf binary.
pub fn read_wharf_exact(reader: &mut impl Read, buf: &mut [u8]) -> Result<()> {
  reader.read_exact(buf).map_err(|e| {
    // Return an InvalidWharfBinary error if an unexpected EOF is encountered,
    // and an IO error for every other case.
    if let io::ErrorKind::UnexpectedEof = e.kind() {
      InvalidWharfBinary::UnexpectedEOF(e).into()
    } else {
      IoError::WharfBinaryReadFailed(e).into()
    }
  })
}

pub trait WharfBinary
where
  Self: Sized,
{
  const MAGIC: u32;
  fn read_without_magic(reader: &mut impl Read) -> Result<Self>;

  fn read(reader: &mut impl Read) -> Result<Self> {
    check_magic_bytes(reader, Self::MAGIC)?;
    Self::read_without_magic(reader)
  }
}
