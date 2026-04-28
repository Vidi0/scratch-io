use crate::errors::{InvalidWharfBinary, IoError, Result};
use crate::magic::check_magic_bytes;

use std::io::{self, BufRead, Read, Write};

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
      InvalidWharfBinary::UnexpectedEOF.into()
    } else {
      IoError::WharfBinaryReadFailed(e).into()
    }
  })
}

pub trait Dump {
  fn dump(&mut self, writer: &mut impl Write) -> Result<()>;
}

pub trait WharfBinary<'reader, R: BufRead + 'reader>: Dump
where
  Self: Sized,
{
  /// The magic bytes of this wharf binary
  const MAGIC: u32;

  /// Decode a wharf binary assuming the magic bytes have already been consumed
  /// from the input stream
  fn read_without_magic(reader: &'reader mut R) -> Result<Self>;

  /// Decode a wharf binary
  ///
  /// If the magic bytes have already been consumed, use [`WharfBinary::read_without_magic`].
  fn read(reader: &'reader mut R) -> Result<Self> {
    // Check the magic bytes
    check_magic_bytes(reader, Self::MAGIC)?;

    // Decode the remaining data
    Self::read_without_magic(reader)
  }
}
