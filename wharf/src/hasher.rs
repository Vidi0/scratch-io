mod errors;

use crate::common::{BLOCK_SIZE, block_count};
use crate::protos;
use crate::signature::{BlockHashIter, MD5_HASH_LENGTH, Md5HashSize};
pub use errors::{BlockHasherError, BlockHasherStatus};

use md5::digest::array::Array;
use md5::{Digest, Md5};
use std::io::Read;

#[derive(Clone, Debug)]
struct InternalHasher {
  hasher: Md5,
  hash_buffer: Array<u8, Md5HashSize>,
}

impl InternalHasher {
  pub fn new() -> Self {
    Self {
      hasher: Md5::new(),
      hash_buffer: Array::<u8, Md5HashSize>::default(),
    }
  }
}

#[derive(Clone, Debug)]
struct FileBlock<'data> {
  pub block_index: usize,

  pub data: &'data [u8],
  pub expected_hash: [u8; MD5_HASH_LENGTH],
}

impl InternalHasher {
  pub fn hash_block(&mut self, block: &FileBlock) -> BlockHasherStatus {
    self.hasher.update(block.data);
    self.hasher.finalize_into_reset(&mut self.hash_buffer);

    if self.hash_buffer == block.expected_hash {
      BlockHasherStatus::Ok
    } else {
      BlockHasherStatus::HashMismatch {
        block_index: block.block_index,
      }
    }
  }
}

pub struct BlockHasher<'cont, 'hash_iter, R> {
  container: &'cont protos::Container,
  entry_index: usize,

  hash_iter: &'hash_iter mut BlockHashIter<R>,
  block_buffer: Box<[u8; BLOCK_SIZE]>,

  internal_hasher: InternalHasher,
}

impl<'cont, 'hash, R> BlockHasher<'cont, 'hash, R> {
  pub fn new(container: &'cont protos::Container, hash_iter: &'hash mut BlockHashIter<R>) -> Self {
    Self {
      container,
      entry_index: 0,

      hash_iter,
      block_buffer: Box::new([0u8; BLOCK_SIZE]),

      internal_hasher: InternalHasher::new(),
    }
  }
}

impl<R> BlockHasher<'_, '_, R>
where
  R: Read,
{
  /// Return the size of the next file in the container
  fn current_file_size(&self) -> Result<u64, BlockHasherError> {
    self
      .container
      .files
      .get(self.entry_index)
      .map(|f| f.size as u64)
      .ok_or(BlockHasherError::RunOutOfFiles {
        file_index: self.entry_index,
      })
  }

  /// This function must be called after the [`BlockHasher`] has just been created
  pub fn skip_files(&mut self, file_count: usize) -> Result<(), String> {
    assert_eq!(self.entry_index, 0);

    for _ in 0..file_count {
      // Skip all the blocks
      let file_size = self.current_file_size()?;
      self.hash_iter.skip_blocks(block_count(file_size))?;
      self.entry_index += 1;
    }

    Ok(())
  }

  /// Hash the next file and verify its integrity against the signature
  ///
  /// Reads the file block by block from `reader`, hashing each block and
  /// comparing it against the expected hash from the signature iterator.
  /// If a hash mismatch is found, the remaining blocks are skipped in the
  /// signature iterator to keep it in sync, and [`BlockHasherStatus::HashMismatch`]
  /// is returned with the index of the first broken block.
  ///
  /// # Returns
  ///
  /// - [`BlockHasherStatus::Ok`] if all blocks match their expected hashes.
  /// - [`BlockHasherStatus::HashMismatch`] if any block fails verification,
  ///   containing the index of the first broken block.
  ///
  /// # Errors
  ///
  /// If the signature iterator runs out of hashes before all blocks are read,
  /// the iterator returns an error, or there is an I/O failure while reading
  /// the file.
  pub fn hash_next_file(
    &mut self,
    reader: &mut impl Read,
  ) -> Result<BlockHasherStatus, BlockHasherError> {
    // Get the next file size and reader
    let file_size = self.current_file_size()?;
    let file_blocks = block_count(file_size);

    self.entry_index += 1;

    let mut read_blocks = 0;
    let mut has_verification_failed = false;
    let mut broken_block_index = 0;

    for block_index in 0..file_blocks as usize {
      if has_verification_failed {
        break;
      }

      // Read the expected hash from the signature
      let expected_hash = self
        .hash_iter
        .next()
        .ok_or(BlockHasherError::MissingHashFromIter)?
        .map(|hash| hash.strong_hash)
        .map_err(BlockHasherError::IterReturnedError)?;

      // Add 1 to the read blocks counter after reading the hash from the iterator
      read_blocks += 1;

      // Calculate how many bytes to read for this block
      let block_buffer = {
        let bytes_remaining = file_size as usize - (block_index * BLOCK_SIZE);
        let block_size = BLOCK_SIZE.min(bytes_remaining);
        &mut self.block_buffer[..block_size]
      };

      // Read the file block into the buffer
      reader
        .read_exact(block_buffer)
        .map_err(BlockHasherError::ReaderFailed)?;

      // Create a FileBlock struct to pass it into the hasher
      let block = FileBlock {
        block_index,
        data: &*block_buffer,
        expected_hash,
      };

      // Hash the data and compare the hashes
      let status = self.internal_hasher.hash_block(&block);
      if let BlockHasherStatus::HashMismatch { block_index } = status {
        has_verification_failed = true;
        broken_block_index = block_index;
      }
    }

    if has_verification_failed {
      self
        .hash_iter
        .skip_blocks(file_blocks - read_blocks)
        .map_err(BlockHasherError::IterReturnedError)?;
    }

    Ok(if has_verification_failed {
      BlockHasherStatus::HashMismatch {
        block_index: broken_block_index,
      }
    } else {
      BlockHasherStatus::Ok
    })
  }
}
