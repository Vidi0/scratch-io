use crate::common::BLOCK_SIZE;
use crate::signature::MD5_HASH_LENGTH;

use std::sync::MutexGuard;

pub struct PoolSlot {
  block_index: usize,

  expected_hash: [u8; MD5_HASH_LENGTH],

  buffer: [u8; BLOCK_SIZE],
  len: usize,
}

impl PoolSlot {
  pub fn empty() -> Self {
    Self {
      block_index: 0,
      expected_hash: [0u8; MD5_HASH_LENGTH],
      buffer: [0u8; BLOCK_SIZE],
      len: 0,
    }
  }

  /// Get a [`RefillBuffer`] from a [`MutexGuard<PoolSlot>`] with the provided `block_index` and `len`
  pub fn get_refill_buffer(
    mut guard: MutexGuard<'_, PoolSlot>,
    slot_index: usize,
    block_index: usize,
    len: usize,
  ) -> RefillBuffer<'_> {
    guard.block_index = block_index;
    guard.len = len;

    RefillBuffer {
      slot: guard,
      slot_index,
    }
  }

  /// Get a [`HashBuffer`] from a [`MutexGuard<PoolSlot>`]
  pub fn get_hash_buffer(guard: MutexGuard<'_, PoolSlot>, slot_index: usize) -> HashBuffer<'_> {
    HashBuffer {
      slot: guard,
      slot_index,
    }
  }
}

pub struct RefillBuffer<'a> {
  slot_index: usize,
  slot: MutexGuard<'a, PoolSlot>,
}

pub struct HashBuffer<'a> {
  slot_index: usize,
  slot: MutexGuard<'a, PoolSlot>,
}

impl RefillBuffer<'_> {
  pub fn slot_index(&self) -> usize {
    self.slot_index
  }

  pub fn set_expected_hash(&mut self, expected_hash: [u8; MD5_HASH_LENGTH]) {
    self.slot.expected_hash = expected_hash;
  }

  pub fn buffer_mut(&mut self) -> &mut [u8] {
    let len = self.slot.len;
    &mut self.slot.buffer[..len]
  }
}

impl HashBuffer<'_> {
  pub fn slot_index(&self) -> usize {
    self.slot_index
  }

  pub fn block_index(&self) -> usize {
    self.slot.block_index
  }

  pub fn expected_hash(&self) -> &[u8; MD5_HASH_LENGTH] {
    &self.slot.expected_hash
  }

  pub fn buffer(&self) -> &[u8] {
    let len = self.slot.len;
    &self.slot.buffer[..len]
  }
}
