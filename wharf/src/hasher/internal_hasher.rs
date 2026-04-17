use super::{BlockHasherStatus, HashBuffer};
use crate::signature::strong_hash::{self, Digest};

#[derive(Clone, Debug)]
pub struct InternalHasher {
  hasher: strong_hash::Hasher,
  hash_buffer: strong_hash::Output,
}

impl InternalHasher {
  pub fn new() -> Self {
    Self {
      hasher: strong_hash::Hasher::new(),
      hash_buffer: strong_hash::Output::default(),
    }
  }
}

impl InternalHasher {
  pub fn hash_data(
    &mut self,
    block_index: usize,
    expected_hash: &[u8; strong_hash::LENGTH],
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
