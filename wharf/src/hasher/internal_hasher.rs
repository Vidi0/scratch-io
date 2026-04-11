use super::{BlockHasherStatus, HashBuffer};
use crate::signature::{MD5_HASH_LENGTH, Md5HashSize};

use md5::digest::array::Array;
use md5::{Digest, Md5};

#[derive(Clone, Debug)]
pub struct InternalHasher {
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

impl InternalHasher {
  pub fn hash_data(
    &mut self,
    block_index: usize,
    expected_hash: &[u8; MD5_HASH_LENGTH],
    buffer: &[u8],
  ) -> BlockHasherStatus {
    self.hasher.update(buffer);
    self.hasher.finalize_into_reset(&mut self.hash_buffer);

    if self.hash_buffer == *expected_hash {
      BlockHasherStatus::Ok
    } else {
      BlockHasherStatus::HashMismatch { block_index }
    }
  }

  pub fn hash_block(&mut self, block: &HashBuffer) -> BlockHasherStatus {
    self.hash_data(block.block_index(), block.expected_hash(), block.buffer())
  }
}
