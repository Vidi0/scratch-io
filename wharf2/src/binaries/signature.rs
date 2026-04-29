use super::{Dump, LendingIterator, WharfBinary};
use crate::decompress::Decompressor;
use crate::errors::Result;
use crate::magic::SIGNATURE_MAGIC;
use crate::protos::{BlockHash, Container, Message, SignatureHeader};

use std::collections::VecDeque;
use std::io::{BufRead, Read};
use std::iter::FusedIterator;

pub struct HashIter<R: Read> {
  reader: R,
  remaining_blocks: u64,
}

impl<R: Read> HashIter<R> {
  fn drain(&mut self) -> Result<()> {
    for hash in self {
      hash?;
    }

    Ok(())
  }
}

impl<R: Read> Iterator for HashIter<R> {
  type Item = Result<BlockHash>;

  /// Decode the next [`BlockHash`] in the stream
  fn next(&mut self) -> Option<Self::Item> {
    if self.remaining_blocks == 0 {
      return None;
    }

    self.remaining_blocks -= 1;
    Some(BlockHash::decode(&mut self.reader))
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    (
      self.remaining_blocks as usize,
      Some(self.remaining_blocks as usize),
    )
  }
}

impl<R: Read> ExactSizeIterator for HashIter<R> {}
impl<R: Read> FusedIterator for HashIter<R> {}

impl<R: Read> Dump for HashIter<R> {
  fn dump(&mut self, writer: &mut impl std::io::Write) -> Result<()> {
    for hash in self {
      hash?.dump(writer)?;
    }

    Ok(())
  }
}

pub struct FileHashIter<R: Read> {
  hash_iter: HashIter<R>,
  remaining_files: VecDeque<u64>,
}

impl<R: Read> FileHashIter<R> {
  fn new(reader: R, container: &Container) -> Self {
    // Create the inner hash iter
    let hash_iter = HashIter {
      reader,
      remaining_blocks: 0,
    };

    // Get the number of blocks of each remaining file
    let remaining_files = container.files.iter().map(|f| f.blocks()).collect();

    Self {
      hash_iter,
      remaining_files,
    }
  }
}

impl<R: Read> LendingIterator for FileHashIter<R> {
  type Item<'a>
    = Result<&'a mut HashIter<R>>
  where
    R: 'a;

  fn next<'a>(&'a mut self) -> Option<Self::Item<'a>> {
    let file_blocks = self.remaining_files.pop_front()?;

    // Skip the blocks that belong to the last file and have not been read
    if let Err(e) = self.hash_iter.drain() {
      return Some(Err(e));
    }

    // Reset the hash iter
    self.hash_iter.remaining_blocks = file_blocks;

    Some(Ok(&mut self.hash_iter))
  }
}

impl<R: Read> Dump for FileHashIter<R> {
  fn dump(&mut self, writer: &mut impl std::io::Write) -> Result<()> {
    while let Some(hash_iter) = self.next() {
      hash_iter?.dump(writer)?;
    }

    Ok(())
  }
}

pub struct Signature<'reader, R: BufRead> {
  header: SignatureHeader,
  container_new: Container,
  hash_iter: FileHashIter<Decompressor<'reader, R>>,
}

impl<R: BufRead> Dump for Signature<'_, R> {
  fn dump(&mut self, writer: &mut impl std::io::Write) -> Result<()> {
    self.header.dump(writer)?;
    self.container_new.dump(writer)?;
    self.hash_iter.dump(writer)
  }
}

impl<'reader, R: BufRead + 'reader> WharfBinary<'reader, R> for Signature<'reader, R> {
  const MAGIC: u32 = SIGNATURE_MAGIC;

  fn read_without_magic(reader: &'reader mut R) -> Result<Self> {
    // Decode the signature header
    let header = SignatureHeader::decode(reader)?;

    // Decompress the remaining stream
    let mut reader = Decompressor::new(reader, header.compression.algorithm)?;

    // Decode the container
    let container_new = Container::decode(&mut reader)?;

    // Create a new file hash iter
    let hash_iter = FileHashIter::new(reader, &container_new);

    Ok(Signature {
      header,
      container_new,
      hash_iter,
    })
  }
}
