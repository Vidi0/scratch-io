mod header;
mod slot;

pub use slot::{HashBuffer, PoolSlot, RefillBuffer};

use header::{Header, PoolStatus, SlotStatus};

use super::BlockHasherStatus;

use parking_lot::{Condvar, Mutex, MutexGuard};

pub struct BufferPool {
  slots: Vec<Mutex<PoolSlot>>,
  header: Header,
}

impl BufferPool {
  pub fn new(pool_size: usize) -> Self {
    assert_ne!(pool_size, 0);

    Self {
      slots: (0..pool_size)
        .map(|index| Mutex::new(PoolSlot::new(index)))
        .collect(),

      header: Header::new(pool_size),
    }
  }

  /// Create a new [`BufferPoolSession`] from this [`BufferPool`] in order to
  /// verify the next file.
  pub fn new_session(&mut self, total_blocks: u64) -> BufferPoolSession<'_> {
    self.header.reset(total_blocks);
    BufferPoolSession {
      slots: &self.slots,
      header: &self.header,
    }
  }
}

pub struct BufferPoolSession<'a> {
  slots: &'a [Mutex<PoolSlot>],
  header: &'a Header,
}

impl BufferPoolSession<'_> {
  /// This function will panic if the slot is currently in use
  fn get_slot(&self, slot_index: usize) -> MutexGuard<'_, PoolSlot> {
    self.slots[slot_index]
      .try_lock()
      .expect("caller ensures slot is not in use")
  }

  fn find_slot(
    &self,
    condvar: &Condvar,
    expected_status: SlotStatus,
    new_status: SlotStatus,
  ) -> Option<MutexGuard<'_, PoolSlot>> {
    let slot_index = {
      // Lock the header
      let status_guard = self.header.get_status_lock();

      // Wait until an available slot can be obtained
      PoolStatus::find_slot(status_guard, condvar, expected_status, new_status)?
    };

    // Obtain the slot
    Some(self.get_slot(slot_index))
  }

  pub fn get_buffer_to_refill(
    &self,
    block_index: usize,
    buffer_len: usize,
  ) -> Option<RefillBuffer<'_>> {
    // Obtain the slot
    let slot_guard = self.find_slot(
      &self.header.refill_ready,
      SlotStatus::WaitingForRefill,
      SlotStatus::Refilling,
    )?;

    Some(PoolSlot::get_refill_buffer(
      slot_guard,
      block_index,
      buffer_len,
    ))
  }

  pub fn get_buffer_to_hash(&self) -> Option<HashBuffer<'_>> {
    // Obtain the slot
    let slot_guard = self.find_slot(
      &self.header.hash_ready,
      SlotStatus::WaitingForHash,
      SlotStatus::Hashing,
    )?;

    Some(PoolSlot::get_hash_buffer(slot_guard))
  }

  pub fn release_refilled_buffer(&self, buffer: RefillBuffer<'_>) {
    let slot_index = buffer.slot_index();

    // Drop the buffer before changing the status
    drop(buffer);

    {
      let mut status = self.header.get_status_lock();
      status.release_slot(slot_index, SlotStatus::WaitingForHash);
    }

    // Wake up the waiting threads
    self.header.hash_ready.notify_one();
  }

  pub fn release_hashed_buffer(&self, buffer: HashBuffer<'_>) {
    let slot_index = buffer.slot_index();

    // Drop the buffer before changing the status
    drop(buffer);

    let has_finished = {
      let mut status = self.header.get_status_lock();
      status.release_slot(slot_index, SlotStatus::WaitingForRefill);

      // Add one to the number of hashed blocks and check
      // whether the verification has finished
      status.add_one_hashed_block()
    };

    if has_finished {
      // Notify all waiting threads to stop
      self.header.refill_ready.notify_all();
      self.header.hash_ready.notify_all();
    } else {
      // Wake up the waiting refill thread
      self.header.refill_ready.notify_one();
    }
  }

  pub fn set_failed(&self, broken_block_index: usize) {
    {
      let mut status = self.header.get_status_lock();
      status.set_failed(broken_block_index);
    }

    // Notify all waiting threads to stop
    self.header.refill_ready.notify_all();
    self.header.hash_ready.notify_all();
  }

  pub fn finished_status(&self) -> BlockHasherStatus {
    let status = self.header.get_status_lock();
    status.finished_status()
  }
}
