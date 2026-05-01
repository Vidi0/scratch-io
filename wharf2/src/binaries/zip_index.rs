use super::{Dump, WharfBinary};
use crate::errors::Result;
use crate::magic::ZIP_INDEX_MAGIC;

use std::fmt::Display;
use std::io::BufRead;
use std::marker::PhantomData;

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
    todo!()
  }
}

impl<'reader, R: BufRead + 'reader> WharfBinary<'reader, R> for ZipIndex<'reader, R> {
  const MAGIC: u32 = ZIP_INDEX_MAGIC;

  fn read_without_magic(_reader: &'reader mut R) -> crate::errors::Result<Self> {
    todo!();
  }
}
