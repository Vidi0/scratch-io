use super::{Dump, WharfBinary};
use crate::decompress::Decompressor;
use crate::errors::{Error, InvalidWharfBinary, Result};
use crate::magic::MANIFEST_MAGIC;
use crate::protos::{ManifestBlockHash, ManifestHeader, Message};

use std::fmt::Display;
use std::io::{BufRead, Read};
use std::iter::FusedIterator;

pub struct ManifestBlockIter<R: Read> {
  reader: R,
  has_finished: bool,
}

impl<R: Read> ManifestBlockIter<R> {
  fn new(reader: R) -> Self {
    Self {
      reader,
      has_finished: false,
    }
  }
}

impl<R: Read> Iterator for ManifestBlockIter<R> {
  type Item = Result<ManifestBlockHash>;

  fn next(&mut self) -> Option<Self::Item> {
    if self.has_finished {
      return None;
    }

    // The manifest block iter may finish at any time (the number of blocks is variable)
    // For that reason, don't error on `InvalidWharfBinary::UnexpectedEOF`, but return `None`
    match ManifestBlockHash::decode(&mut self.reader) {
      Ok(p) => Some(Ok(p)),
      Err(Error::InvalidWharfBinary(InvalidWharfBinary::UnexpectedEOF)) => {
        self.has_finished = true;
        None
      }
      Err(e) => Some(Err(e)),
    }
  }
}

impl<R: Read> FusedIterator for ManifestBlockIter<R> {}

impl<R: Read> Dump for ManifestBlockIter<R> {
  fn dump(&mut self, writer: &mut impl std::io::Write) -> Result<()> {
    for hash in self {
      hash?.dump(writer)?;
    }

    Ok(())
  }
}

pub struct Manifest<'reader, R: BufRead> {
  header: ManifestHeader,
  block_iter: ManifestBlockIter<Decompressor<'reader, R>>,
}

impl<R: BufRead> Display for Manifest<'_, R> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "wharf manifest file ({})
  algorithm: {}",
      self.header.compression, self.header.algorithm
    )
  }
}

impl<R: BufRead> Dump for Manifest<'_, R> {
  fn dump(&mut self, writer: &mut impl std::io::Write) -> Result<()> {
    self.header.dump(writer)?;
    self.block_iter.dump(writer)
  }
}

impl<'reader, R: BufRead + 'reader> WharfBinary<'reader, R> for Manifest<'reader, R> {
  const MAGIC: u32 = MANIFEST_MAGIC;

  fn read_without_magic(reader: &'reader mut R) -> crate::errors::Result<Self> {
    // Decode the manifest header
    let header = ManifestHeader::decode(reader)?;

    // Decompress the remaining stream
    let reader = Decompressor::new(reader, header.compression)?;

    // Create a manifest block iter
    let block_iter = ManifestBlockIter::new(reader);

    Ok(Self { header, block_iter })
  }
}
