pub mod manifest;
pub mod patch;
pub mod signature;
pub mod wounds;
pub mod zip_index;

use crate::errors::{InvalidWharfBinary, IoError, Result};
use crate::magic::check_magic_bytes;

use std::fmt::Display;
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

pub trait LendingIterator {
  type Item<'a>
  where
    Self: 'a;

  fn next<'a>(&'a mut self) -> Option<Self::Item<'a>>;
}

/// Serialize the contents of a wharf binary to a human-readable text
/// representation
///
/// Implemented by wharf binary types and their sub-iterators to support the
/// `dump` command, which prints the decoded contents of a wharf binary for
/// inspection.
pub trait Dump {
  fn dump(&mut self, writer: &mut impl Write) -> Result<()>;
}

/// A wharf binary file that can be read from a buffered stream
///
/// Implemented by [`Signature`](signature::Signature), [`Patch`](patch::Patch),
/// [`Manifest`](manifest::Manifest), [`Wounds`](wounds::Wounds), and
/// [`ZipIndex`](zip_index::ZipIndex). To read any of the binaries, see
/// [`WharfBinary::read`]
///
///
/// Implementors must also implement [`Dump`] for printing decoded contents
/// and [`Display`] for printing a human-readable summary of the binary.
pub trait WharfBinary<'reader, R: BufRead + 'reader>
where
  Self: Sized,
  Self: Dump + Display,
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
