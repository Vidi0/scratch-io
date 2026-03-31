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
  status: Mutex<Vec<SlotStatus>>,
  buffers: Vec<Mutex<Slot>>,
}

impl BufferPool {
  pub fn new(size: usize) -> Self {
    Self {
      status: Mutex::new(vec![SlotStatus::WaitingForRefill; size]),
      buffers: std::iter::repeat_with(|| Mutex::new(Slot::empty()))
        .take(size)
        .collect(),
    }
  }

  fn get_buffer<K>(&self) -> Option<BufferHandle<'_, K>>
  where
    K: buffer_handle::Kind,
  {
    let mut status = self.status.lock().unwrap();

    for (index, s) in status.iter_mut().enumerate() {
      if *s == K::expected() {
        // Getting the buffer won't lock because the status is WaitingForRefill
        // or WaitingForHash, so no other thread should have it.
        let guard = self.buffers[index]
          .try_lock()
          .expect("Buffer lock failed despite status indicating availability");

        // Set the status to Refilling or Hashing so no other thread can try obtaining it
        *s = K::current();

        return Some(BufferHandle {
          guard,
          slot_index: index,
          _kind: PhantomData,
        });
      }
    }

    None
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
    let index = buffer.slot_index;
    // release the MutexGuard FIRST before obtaining the status
    drop(buffer);

    let mut status = self.status.lock().unwrap();
    status[index] = K::next();
  }
}
