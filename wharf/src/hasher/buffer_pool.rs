use crate::common::BLOCK_SIZE;
use crate::signature::MD5_HASH_LENGTH;

use std::marker::PhantomData;
use std::sync::{Mutex, MutexGuard};

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum SlotStatus {
  WaitingForRefill,
  Refilling,
  WaitingForHash,
  Hashing,
}

#[derive(Debug, Clone)]
struct Slot {
  expected_hash: [u8; MD5_HASH_LENGTH],
  data: [u8; BLOCK_SIZE],
  len: usize,
  block_index: usize,
}

impl Slot {
  pub fn empty() -> Self {
    Self {
      expected_hash: [0u8; MD5_HASH_LENGTH],
      data: [0u8; BLOCK_SIZE],
      len: 0,
      block_index: 0,
    }
  }
}

mod buffer_handle {
  use super::SlotStatus;

  #[derive(Clone, Copy, Debug)]
  pub struct Refill;

  #[derive(Clone, Copy, Debug)]
  pub struct Hash;

  pub trait Kind {
    fn expected() -> SlotStatus;
    fn current() -> SlotStatus;
    fn next() -> SlotStatus;
  }

  impl Kind for Refill {
    fn expected() -> SlotStatus {
      SlotStatus::WaitingForRefill
    }

    fn current() -> SlotStatus {
      SlotStatus::Refilling
    }

    fn next() -> SlotStatus {
      SlotStatus::WaitingForHash
    }
  }

  impl Kind for Hash {
    fn expected() -> SlotStatus {
      SlotStatus::WaitingForHash
    }

    fn current() -> SlotStatus {
      SlotStatus::Hashing
    }

    fn next() -> SlotStatus {
      SlotStatus::WaitingForRefill
    }
  }
}

#[derive(Debug)]
struct PoolStatus {
  slots: Mutex<Vec<SlotStatus>>,
}

impl PoolStatus {
  pub fn new(size: usize) -> Self {
    Self {
      slots: Mutex::new(vec![SlotStatus::WaitingForRefill; size]),
    }
  }

  /// Return the index of the first slot available for the provided buffer kind
  /// and set it as occupied.
  ///
  /// If this function returns `Some(slot_index)`, the buffer slot in the provided
  /// index in the pool can be obtained without locking because the status was
  /// WaitingForRefill or WaitingForHash, so no other thread had it; and the status
  /// was now set to Refill or Hash to prevent other thread from claiming it.
  pub fn claim_empty_slot<K>(&self) -> Option<usize>
  where
    K: buffer_handle::Kind,
  {
    let mut slots = self.slots.lock().unwrap();

    for (index, s) in slots.iter_mut().enumerate() {
      if *s == K::expected() {
        // Set the status to Refilling or Hashing so no other thread can try obtaining it
        *s = K::current();

        return Some(index);
      }
    }

    None
  }

  /// This function must be called AFTER releasing the corresponding buffer on the pool
  pub fn free_slot<K>(&self, slot_index: usize)
  where
    K: buffer_handle::Kind,
  {
    let mut slots = self.slots.lock().unwrap();
    slots[slot_index] = K::next();
  }
}

#[derive(Debug)]
pub struct BufferHandle<'a, K> {
  guard: MutexGuard<'a, Slot>,
  slot_index: usize,
  _kind: PhantomData<K>,
}

pub type RefillBuffer<'a> = BufferHandle<'a, buffer_handle::Refill>;
pub type HashBuffer<'a> = BufferHandle<'a, buffer_handle::Hash>;

impl RefillBuffer<'_> {
  fn set_block_index(&mut self, block_index: usize) {
    self.guard.block_index = block_index;
  }

  fn set_buffer_len(&mut self, len: usize) {
    self.guard.len = len;
  }
}

impl RefillBuffer<'_> {
  pub fn buffer_mut(&mut self) -> &mut [u8] {
    let len = self.guard.len;
    &mut self.guard.data[..len]
  }

  pub fn set_expected_hash(&mut self, expected_hash: [u8; MD5_HASH_LENGTH]) {
    self.guard.expected_hash = expected_hash;
  }
}

impl HashBuffer<'_> {
  pub fn block_index(&self) -> usize {
    self.guard.block_index
  }

  pub fn buffer(&self) -> &[u8] {
    let len = self.guard.len;
    &self.guard.data[..len]
  }

  pub fn expected_hash(&self) -> &[u8; MD5_HASH_LENGTH] {
    &self.guard.expected_hash
  }
}

#[derive(Debug)]
pub struct BufferPool {
  status: PoolStatus,
  buffers: Vec<Mutex<Slot>>,
}

impl BufferPool {
  pub fn new(size: usize) -> Self {
    Self {
      status: PoolStatus::new(size),
      buffers: std::iter::repeat_with(|| Mutex::new(Slot::empty()))
        .take(size)
        .collect(),
    }
  }

  fn get_buffer<K>(&self) -> Option<BufferHandle<'_, K>>
  where
    K: buffer_handle::Kind,
  {
    let slot_index = self.status.claim_empty_slot::<K>()?;

    let guard = self.buffers[slot_index]
      .try_lock()
      .expect("Buffer lock failed despite status indicating availability");

    Some(BufferHandle {
      guard,
      slot_index,
      _kind: PhantomData,
    })
  }

  pub fn get_buffer_to_refill(
    &self,
    block_index: usize,
    buffer_len: usize,
  ) -> Option<RefillBuffer<'_>> {
    if let Some(mut buffer) = self.get_buffer::<buffer_handle::Refill>() {
      buffer.set_block_index(block_index);
      buffer.set_buffer_len(buffer_len);
      Some(buffer)
    } else {
      None
    }
  }

  pub fn get_buffer_to_hash(&self) -> Option<HashBuffer<'_>> {
    self.get_buffer::<buffer_handle::Hash>()
  }

  /// Take ownership of the buffer handle in order to drop the guard and allow the
  /// buffer to be taken by other thread.
  pub fn drop_buffer<K>(&self, buffer: BufferHandle<K>)
  where
    K: buffer_handle::Kind,
  {
    let slot_index = buffer.slot_index;
    // Release the buffer slot FIRST before updating the status
    drop(buffer);

    self.status.free_slot::<K>(slot_index);
  }
}
