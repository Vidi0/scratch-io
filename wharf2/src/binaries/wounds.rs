use super::{Dump, WharfBinary};
use crate::decompress::Decompressor;
use crate::errors::{Error, InvalidWharfBinary, Result};
use crate::magic::WOUNDS_MAGIC;
use crate::protos::{CompressionSettings, Container, Message, Wound, WoundsHeader};

use std::fmt::Display;
use std::io::{BufRead, Read};
use std::iter::FusedIterator;

/// Iterator over an unknown amount of wounds
pub struct WoundsIter<R: Read> {
  reader: R,
  has_finished: bool,
}

impl<R: Read> WoundsIter<R> {
  fn new(reader: R) -> Self {
    Self {
      reader,
      has_finished: false,
    }
  }
}

impl<R: Read> Iterator for WoundsIter<R> {
  type Item = Result<Wound>;

  /// Decode the next [`Wound`] in the stream
  fn next(&mut self) -> Option<Self::Item> {
    if self.has_finished {
      return None;
    }

    // The wounds iter may finish at any time (the number of wounds is variable
    // and cannot be determined only by looking at the container)
    //
    // For that reason, don't error on `InvalidWharfBinary::UnexpectedEOF`,
    // but return `None`
    match Wound::decode(&mut self.reader) {
      Ok(p) => Some(Ok(p)),
      Err(Error::InvalidWharfBinary(InvalidWharfBinary::UnexpectedEOF)) => {
        self.has_finished = true;
        None
      }
      Err(e) => Some(Err(e)),
    }
  }
}

impl<R: Read> FusedIterator for WoundsIter<R> {}

impl<R: Read> Dump for WoundsIter<R> {
  fn dump(&mut self, writer: &mut impl std::io::Write) -> Result<()> {
    for wound in self {
      wound?.dump(writer)?;
    }

    Ok(())
  }
}

pub struct Wounds<'reader, R: BufRead> {
  header: WoundsHeader,
  container_new: Container,
  wounds_iter: WoundsIter<Decompressor<'reader, R>>,
}

impl<R: BufRead> Display for Wounds<'_, R> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    writeln!(
      f,
      "wharf wounds file ({})
  new: {}",
      // Wharf wounds binaries are always uncompressed
      CompressionSettings::None,
      self.container_new
    )
  }
}

impl<R: BufRead> Dump for Wounds<'_, R> {
  fn dump(&mut self, writer: &mut impl std::io::Write) -> Result<()> {
    self.header.dump(writer)?;
    self.container_new.dump(writer)?;
    self.wounds_iter.dump(writer)
  }
}

impl<'reader, R: BufRead + 'reader> WharfBinary<'reader, R> for Wounds<'reader, R> {
  const MAGIC: u32 = WOUNDS_MAGIC;

  fn read_without_magic(reader: &'reader mut R) -> Result<Self> {
    // Decode the wounds header
    let header = WoundsHeader::decode(reader)?;

    // The wounds wharf binary format is not compressed
    // Decompress the remaining stream with None as the algorithm
    let mut reader = Decompressor::new(reader, CompressionSettings::None)?;

    // Decode the container
    let container_new = Container::decode(&mut reader)?;

    // Create a new wounds iter
    let wounds_iter = WoundsIter::new(reader);

    Ok(Wounds {
      header,
      container_new,
      wounds_iter,
    })
  }
}
