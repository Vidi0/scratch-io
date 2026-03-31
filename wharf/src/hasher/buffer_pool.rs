use super::errors::BlockHasherStatus;
use super::verification_status::VerificationStatus;
use crate::common::BLOCK_SIZE;
use crate::signature::MD5_HASH_LENGTH;

use std::marker::PhantomData;
use std::sync::{Condvar, Mutex, MutexGuard};

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
  use super::{SlotStatus, SlotWaiters};
  use std::sync::Condvar;

  #[derive(Clone, Copy, Debug)]
  pub struct Refill;

  #[derive(Clone, Copy, Debug)]
  pub struct Hash;

  pub trait Kind {
    fn expected() -> SlotStatus;
    fn current() -> SlotStatus;
    fn next() -> SlotStatus;

    fn current_waiter(waiters: &SlotWaiters) -> &Condvar;
    fn next_waiter(waiters: &SlotWaiters) -> &Condvar;
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

    fn current_waiter(waiters: &SlotWaiters) -> &Condvar {
      &waiters.refill_ready
    }

    fn next_waiter(waiters: &SlotWaiters) -> &Condvar {
      &waiters.hash_ready
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

    fn current_waiter(waiters: &SlotWaiters) -> &Condvar {
      &waiters.hash_ready
    }

    fn next_waiter(waiters: &SlotWaiters) -> &Condvar {
      &waiters.refill_ready
    }
  }
}

#[derive(Debug)]
pub struct SlotWaiters {
  /// Notified when a slot becomes WaitingForHash: wakes hasher threads
  pub hash_ready: Condvar,
  /// Notified when a slot becomes WaitingForRefill: wakes the IO thread
  pub refill_ready: Condvar,
}

impl SlotWaiters {
  pub fn new() -> Self {
    Self {
      hash_ready: Condvar::new(),
      refill_ready: Condvar::new(),
    }
  }

  pub fn notify_all(&self) {
    self.hash_ready.notify_all();
    self.refill_ready.notify_all();
  }
}

#[derive(Debug)]
struct PoolStatus {
  status: VerificationStatus,
  slots: Mutex<Vec<SlotStatus>>,
  waiters: SlotWaiters,
}

impl PoolStatus {
  pub fn new(size: usize) -> Self {
    Self {
      status: VerificationStatus::new(0),
      slots: Mutex::new(vec![SlotStatus::WaitingForRefill; size]),
      waiters: SlotWaiters::new(),
    }
  }

  /// Reset the [`PoolStatus`], allowing the pool to be reused to hash another file
  pub fn reset(&mut self, total_blocks: u64) {
    self.status = VerificationStatus::new(total_blocks);

    // The slots must be all empty
    // It must not lock or else it's everything messed up, so use try_lock to panic on lock
    let slots = self.slots.try_lock().unwrap();
    for slot in slots.iter() {
      assert_eq!(*slot, SlotStatus::WaitingForRefill);
    }
  }

  /// Set the status as failed and signal all waiting threads to exit
  pub fn set_failed(&self, broken_block_index: usize) {
    self.status.set_failed(broken_block_index);
    self.waiters.notify_all();
  }

  pub fn add_one_hashed_block(&self) {
    if self.status.add_one_hashed_block() {
      self.waiters.notify_all();
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

    loop {
      if self.status.has_finished() {
        return None;
      }

      for (index, s) in slots.iter_mut().enumerate() {
        if *s == K::expected() {
          // Set the status to Refilling or Hashing so no other thread can try obtaining it
          *s = K::current();

          return Some(index);
        }
      }

      // All slots are occupied: sleep until a one is released
      slots = K::current_waiter(&self.waiters).wait(slots).unwrap();
    }
  }

  /// This function must be called AFTER releasing the corresponding buffer on the pool
  pub fn free_slot<K>(&self, slot_index: usize)
  where
    K: buffer_handle::Kind,
  {
    let mut slots = self.slots.lock().unwrap();
    slots[slot_index] = K::next();

    // Drop the slots guard before notifying the waiters
    drop(slots);

    // Notify one of the waiters for the next stage that the buffer has been freed
    K::next_waiter(&self.waiters).notify_one();
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

pub struct BufferPool {
  status: PoolStatus,
  buffers: Vec<Mutex<Slot>>,
}

impl BufferPool {
  /// Before using the [`BufferPool`], [`BufferPool::reset`] must be called first.
  pub fn new(size: usize) -> Self {
    Self {
      status: PoolStatus::new(size),
      buffers: std::iter::repeat_with(|| Mutex::new(Slot::empty()))
        .take(size)
        .collect(),
    }
  }

  /// Reset the [`PoolStatus`] allowing the pool to be reused to hash another file
  pub fn reset(&mut self, total_blocks: u64) {
    self.status.reset(total_blocks);
  }

  pub fn set_failed(&self, broken_block_index: usize) {
    self.status.set_failed(broken_block_index);
  }

  pub fn has_failed(&self) -> bool {
    self.status.status.has_failed()
  }

  pub fn blocks_read(&self) -> u64 {
    self.status.status.blocks_read()
  }

  pub fn finished_status(&self) -> BlockHasherStatus {
    self.status.status.finished_status()
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
    let mut buffer = self.get_buffer::<buffer_handle::Refill>()?;

    buffer.set_block_index(block_index);
    buffer.set_buffer_len(buffer_len);

    self.status.status.add_one_read_block();

    Some(buffer)
  }

  pub fn get_buffer_to_hash(&self) -> Option<HashBuffer<'_>> {
    self.get_buffer::<buffer_handle::Hash>()
  }

  /// Take ownership of the buffer handle in order to drop the guard and allow the
  /// buffer to be taken by other thread.
  fn drop_buffer<K>(&self, buffer: BufferHandle<K>)
  where
    K: buffer_handle::Kind,
  {
    let slot_index = buffer.slot_index;
    // Release the buffer slot FIRST before updating the status
    drop(buffer);

    self.status.free_slot::<K>(slot_index);
  }

  pub fn drop_refilled_buffer(&self, buffer: RefillBuffer) {
    self.drop_buffer(buffer);
  }

  pub fn drop_hashed_buffer(&self, buffer: HashBuffer) {
    self.status.add_one_hashed_block();
    self.drop_buffer(buffer);
  }
}
