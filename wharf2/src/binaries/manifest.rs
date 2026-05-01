use super::{Dump, WharfBinary};
use crate::errors::Result;
use crate::magic::MANIFEST_MAGIC;

use std::fmt::Display;
use std::io::BufRead;
use std::marker::PhantomData;

pub struct Manifest<'reader, R: BufRead> {
  _marker: PhantomData<&'reader mut R>,
}

impl<R: BufRead> Display for Manifest<'_, R> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "wharf manifest file")
  }
}

impl<R: BufRead> Dump for Manifest<'_, R> {
  fn dump(&mut self, _writer: &mut impl std::io::Write) -> Result<()> {
    todo!()
  }
}

impl<'reader, R: BufRead + 'reader> WharfBinary<'reader, R> for Manifest<'reader, R> {
  const MAGIC: u32 = MANIFEST_MAGIC;

  fn read_without_magic(_reader: &'reader mut R) -> crate::errors::Result<Self> {
    todo!();
  }
}
