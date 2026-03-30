use super::BlockHasherStatus;
use super::block_buffer::FileBlock;
use crate::signature::Md5HashSize;

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
  pub fn hash_block(&mut self, block: &FileBlock) -> BlockHasherStatus {
    self.hasher.update(block.data.buffer());
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
