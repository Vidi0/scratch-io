mod errors;
pub mod writer;

use crate::container::BLOCK_SIZE;
use crate::signature::read::BlockHashIter;
use errors::BlockHasherError;

use md5::digest::{OutputSizeUser, generic_array::GenericArray, typenum::Unsigned};
use md5::{Digest, Md5};
use std::io::Read;

type Md5HashSize = <md5::Md5 as OutputSizeUser>::OutputSize;
pub const MD5_HASH_LENGTH: usize = Md5HashSize::USIZE;

pub struct BlockHasher<'a, R> {
  hash_iter: &'a mut BlockHashIter<R>,
  written_bytes: usize,
  hasher: Md5,
  hash_buffer: GenericArray<u8, Md5HashSize>,
  first_block: bool,
}

impl<'a, R> BlockHasher<'a, R> {
  pub fn new(hash_iter: &'a mut BlockHashIter<R>) -> Self {
    Self {
      hash_iter,
      written_bytes: 0,
      hasher: Md5::new(),
      hash_buffer: GenericArray::<u8, Md5HashSize>::default(),
      first_block: true,
    }
  }
}

impl<'a, R: Read> BlockHasher<'a, R> {
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
        self.finalize_block()?;
      }
    }

    Ok(())
  }

  pub fn finalize_block(&mut self) -> Result<(), BlockHasherError> {
    // Skip hashing if the current block is empty
    // However, wharf saves an empty hash for an empty file,
    // so ensure this is not the first block before skipping
    if self.written_bytes == 0 && !self.first_block {
      return Ok(());
    }

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

    // Reset hasher variables
    self.first_block = false;
    self.written_bytes = 0;

    Ok(())
  }

  pub fn finalize_block_and_reset(&mut self) -> Result<(), BlockHasherError> {
    self.finalize_block()?;

    // Reset the hasher variables
    self.first_block = true;

    // Setting these variables isn't needed because calling
    // self.finalize_block already sets them:

    //self.written_bytes = 0;
    //self.hasher.reset();

    Ok(())
  }
}
