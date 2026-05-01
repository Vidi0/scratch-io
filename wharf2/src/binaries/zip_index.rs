use super::{Dump, WharfBinary};
use crate::errors::Result;
use crate::magic::ZIP_INDEX_MAGIC;

use std::fmt::Display;
use std::io::BufRead;
use std::marker::PhantomData;

/// A wharf zip index file (`.pzi`)
///
/// Zip index files were designed to pre-cache the central directory of a
/// build's `.zip` archive, allowing the `ArchiveHealer` to seek directly to
/// individual entries in a remote archive over HTTP without fetching the end of
/// the file first.
///
/// The magic number for this format is defined in the wharf constants, but no
/// implementation was ever published. This type is a stub and all operations
/// on it will panic.
///
/// # References
///
/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L26>
pub struct ZipIndex<'reader, R: BufRead> {
  _marker: PhantomData<&'reader mut R>,
}

impl<R: BufRead> Display for ZipIndex<'_, R> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "wharf zip index file")
  }
}

impl<R: BufRead> Dump for ZipIndex<'_, R> {
  fn dump(&mut self, _writer: &mut impl std::io::Write) -> Result<()> {
    unimplemented!()
  }
}

impl<'reader, R: BufRead + 'reader> WharfBinary<'reader, R> for ZipIndex<'reader, R> {
  const MAGIC: u32 = ZIP_INDEX_MAGIC;

  fn read_without_magic(_reader: &'reader mut R) -> crate::errors::Result<Self> {
    unimplemented!();
  }
}
