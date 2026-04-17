use crate::common::BLOCK_SIZE;
use crate::signature::strong_hash;

use parking_lot::MutexGuard;

pub struct PoolSlot {
  index: usize,

  block_index: usize,

  expected_hash: [u8; strong_hash::LENGTH],

  buffer: Box<[u8; BLOCK_SIZE]>,
  len: usize,
}

impl PoolSlot {
  pub fn new(index: usize) -> Self {
    Self {
      index,
      block_index: 0,
      expected_hash: [0u8; strong_hash::LENGTH],
      buffer: Box::new([0u8; BLOCK_SIZE]),
      len: 0,
    }
  }

  /// Get a [`RefillBuffer`] from a [`MutexGuard<PoolSlot>`] with the provided `block_index` and `len`
  pub fn get_refill_buffer(
    mut guard: MutexGuard<'_, PoolSlot>,
    block_index: usize,
    len: usize,
  ) -> RefillBuffer<'_> {
    guard.block_index = block_index;
    guard.len = len;

    RefillBuffer(guard)
  }

  /// Get a [`HashBuffer`] from a [`MutexGuard<PoolSlot>`]
  pub fn get_hash_buffer(guard: MutexGuard<'_, PoolSlot>) -> HashBuffer<'_> {
    HashBuffer(guard)
  }

  /// Get the raw buffer that composes this [`PoolSlot`]
  pub fn buffer_mut(&mut self) -> &mut [u8; BLOCK_SIZE] {
    &mut self.buffer
  }
}

pub struct RefillBuffer<'a>(MutexGuard<'a, PoolSlot>);
pub struct HashBuffer<'a>(MutexGuard<'a, PoolSlot>);

impl RefillBuffer<'_> {
  pub fn slot_index(&self) -> usize {
    self.0.index
  }

  pub fn set_expected_hash(&mut self, expected_hash: [u8; strong_hash::LENGTH]) {
    self.0.expected_hash = expected_hash;
  }

  pub fn buffer_mut(&mut self) -> &mut [u8] {
    let len = self.0.len;
    &mut self.0.buffer[..len]
  }
}

impl HashBuffer<'_> {
  pub fn slot_index(&self) -> usize {
    self.0.index
  }

  pub fn block_index(&self) -> usize {
    self.0.block_index
  }

  pub fn expected_hash(&self) -> &[u8; strong_hash::LENGTH] {
    &self.0.expected_hash
  }

  pub fn buffer(&self) -> &[u8] {
    let len = self.0.len;
    &self.0.buffer[..len]
  }
}
