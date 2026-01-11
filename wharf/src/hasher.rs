mod errors;
pub mod writer;

use crate::container::BLOCK_SIZE;
use crate::signature::read::BlockHashIter;
pub use errors::BlockHasherError;

use md5::digest::{OutputSizeUser, generic_array::GenericArray, typenum::Unsigned};
use md5::{Digest, Md5};
use std::io::Read;

type Md5HashSize = <md5::Md5 as OutputSizeUser>::OutputSize;
pub const MD5_HASH_LENGTH: usize = Md5HashSize::USIZE;

pub struct BlockHasher<'a, R> {
  hash_iter: &'a mut BlockHashIter<R>,
  hasher: Md5,
  hash_buffer: GenericArray<u8, Md5HashSize>,
  written_bytes: usize,
  first_block: bool,
  blocks_since_reset: u64,
}

impl<'a, R> BlockHasher<'a, R> {
  pub fn new(hash_iter: &'a mut BlockHashIter<R>) -> Self {
    Self {
      hash_iter,
      hasher: Md5::new(),
      hash_buffer: GenericArray::<u8, Md5HashSize>::default(),
      written_bytes: 0,
      first_block: true,
      blocks_since_reset: 0,
    }
  }

  /// Reset this hasher, allowing it to hash another file
  pub fn reset(&mut self) {
    self.hasher.reset();
    self.written_bytes = 0;
    self.first_block = true;
    self.blocks_since_reset = 0;
  }

  /// Return the number of blocks hashed since this hasher
  /// was last reset
  #[inline]
  #[must_use]
  pub fn blocks_since_reset(&self) -> u64 {
    self.blocks_since_reset
  }

  /// Return the number of bytes that were passed into the
  /// hasher but didn't fill the current block
  #[inline]
  #[must_use]
  pub fn written_bytes(&self) -> usize {
    self.written_bytes
  }
}

impl<'a, R: Read> BlockHasher<'a, R> {
  /// Update the hahser with new data
  pub fn update(&mut self, buf: &[u8]) -> Result<(), BlockHasherError> {
    let mut offset: usize = 0;

    while offset < buf.len() {
      // Get the next buffer slice
      let block_remaining = BLOCK_SIZE as usize - self.written_bytes;
      let to_take = block_remaining.min(buf.len() - offset);
      let slice = &buf[offset..offset + to_take];

      // Update the hasher
      self.hasher.update(slice);

      // Update internal counters
      offset += to_take;
      self.written_bytes += to_take;

      if self.written_bytes == BLOCK_SIZE as usize {
        // Chunk completed
        self.blocks_since_reset += 1;
        self.finalize_block()?;
      }
    }

    Ok(())
  }

  /// Finalize the current data in the hasher and check the current block
  ///
  /// Don't hash the block if it's empty AND it isn't the first one
  pub fn finalize_block(&mut self) -> Result<(), BlockHasherError> {
    // Skip hashing if the current block is empty
    // However, wharf saves an empty hash for an empty file,
    // so ensure this is not the first block before skipping
    if self.written_bytes == 0 && !self.first_block {
      return Ok(());
    }

    // Reset hasher variables
    self.first_block = false;
    self.written_bytes = 0;

    // Calculate the hash
    self.hasher.finalize_into_reset(&mut self.hash_buffer);

    // Get the next hash from the iterator
    let next_hash = self
      .hash_iter
      .next()
      .ok_or(BlockHasherError::MissingHashFromIter)?
      .map_err(BlockHasherError::IterReturnedError)?;

    // Compare the hashes
    if *self.hash_buffer != *next_hash.strong_hash {
      return Err(BlockHasherError::HashMismatch {
        expected: next_hash.strong_hash,
        found: self.hash_buffer.into(),
      });
    }

    Ok(())
  }

  /// Skip the provied number of blocks and reset the hasher to
  /// allow hashing the next file
  ///
  /// This function should be called after a failed call to update
  pub fn skip_blocks(&mut self, blocks_to_skip: u64) -> Result<(), String> {
    self.hash_iter.skip_blocks(blocks_to_skip)
  }
}
