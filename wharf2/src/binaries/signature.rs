use super::{Dump, LendingIterator, WharfBinary};
use crate::decompress::Decompressor;
use crate::errors::Result;
use crate::magic::SIGNATURE_MAGIC;
use crate::protos::{BlockHash, Container, Message, SignatureHeader};

use std::fmt::Display;
use std::io::{BufRead, Read};
use std::iter::FusedIterator;
use std::ops::Range;

/// Iterator over the [`BlockHash`] messages for a single file in a signature
///
/// Yields exactly `remaining_blocks` hashes before returning `None`,
/// corresponding to the fixed-size blocks of one file in the container.
/// Implements [`ExactSizeIterator`] since the block count is known up front
/// from the container metadata.
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

/// Lending iterator over per-file [`HashIter`]s in a signature stream
///
/// Advances through the container's files in order, yielding a [`HashIter`]
/// scoped to each file's blocks. When [`LendingIterator::next`] is called,
/// any unread blocks from the previous file are drained before the iterator
/// is reset for the next file, keeping the underlying reader in sync.
pub struct FileHashIter<R: Read> {
  hash_iter: HashIter<R>,

  file_indexes: Range<usize>,
  file_blocks: Vec<u64>,
}

impl<R: Read> FileHashIter<R> {
  fn new(reader: R, container: &Container) -> Self {
    // Create the inner hash iter
    let hash_iter = HashIter {
      reader,
      remaining_blocks: 0,
    };

    // Get the number of blocks of each file
    let file_blocks: Vec<u64> = container.files.iter().map(|f| f.blocks()).collect();

    Self {
      hash_iter,
      file_indexes: 0..file_blocks.len(),
      file_blocks,
    }
  }
}

impl<R: Read> LendingIterator for FileHashIter<R> {
  type Item<'a>
    = Result<(usize, &'a mut HashIter<R>)>
  where
    R: 'a;

  fn next<'a>(&'a mut self) -> Option<Self::Item<'a>> {
    let file_index = self.file_indexes.next()?;
    let file_blocks = self.file_blocks[file_index];

    // Skip the blocks that belong to the last file and have not been read
    if let Err(e) = self.hash_iter.drain() {
      return Some(Err(e));
    }

    // Reset the hash iter
    self.hash_iter.remaining_blocks = file_blocks;

    Some(Ok((file_index, &mut self.hash_iter)))
  }
}

impl<R: Read> Dump for FileHashIter<R> {
  fn dump(&mut self, writer: &mut impl std::io::Write) -> Result<()> {
    while let Some(item) = self.next() {
      match item {
        Ok((_file_index, hash_iter)) => hash_iter.dump(writer)?,
        Err(e) => return Err(e),
      }
    }

    Ok(())
  }
}

/// A wharf signature file (`.pws`)
///
/// A signature contains the new container describing the expected filesystem
/// state, followed by MD5 and Adler-32 block hashes for every fixed-size block
/// of every file in the container.
///
/// Signatures are used both for verifying the integrity of an installed build
/// and as the basis for computing patches between builds.
///
/// # References
///
/// <https://docs.itch.zone/wharf/master/file-formats/signatures.html>
pub struct Signature<'reader, R: BufRead> {
  header: SignatureHeader,
  container_new: Container,
  hash_iter: FileHashIter<Decompressor<'reader, R>>,
}

impl<R: BufRead> Display for Signature<'_, R> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "wharf signature file ({})
  new: {}",
      self.header.compression, self.container_new
    )
  }
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
    let mut reader = Decompressor::new(reader, header.compression)?;

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
