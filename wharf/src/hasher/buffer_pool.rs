mod header;
mod slot;

pub use slot::{HashBuffer, PoolSlot, RefillBuffer};

use header::{Header, PoolStatus, SlotStatus};

use super::BlockHasherStatus;

use parking_lot::{Mutex, MutexGuard};

pub struct BufferPool {
  slots: Vec<Mutex<PoolSlot>>,
  header: Header,
}

impl BufferPool {
  pub fn new(pool_size: usize) -> Self {
    Self {
      slots: std::iter::repeat_with(|| Mutex::new(PoolSlot::empty()))
        .take(pool_size)
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

  pub fn get_buffer_to_refill(
    &self,
    block_index: usize,
    buffer_len: usize,
  ) -> Option<RefillBuffer<'_>> {
    let slot_index = {
      let status_guard = self.header.get_status_lock();

      // Wait until an available slot can be obtained
      PoolStatus::find_slot(
        status_guard,
        &self.header.refill_ready,
        SlotStatus::WaitingForRefill,
        SlotStatus::Refilling,
      )?
    };

    // Obtain the slot
    let slot_guard = self.get_slot(slot_index);

    Some(PoolSlot::get_refill_buffer(
      slot_guard,
      slot_index,
      block_index,
      buffer_len,
    ))
  }

  pub fn get_buffer_to_hash(&self) -> Option<HashBuffer<'_>> {
    let slot_index = {
      let status_guard = self.header.get_status_lock();

      // Wait until an available slot can be obtained
      PoolStatus::find_slot(
        status_guard,
        &self.header.hash_ready,
        SlotStatus::WaitingForHash,
        SlotStatus::Hashing,
      )?
    };

    // Obtain the slot
    let slot_guard = self.get_slot(slot_index);
    Some(PoolSlot::get_hash_buffer(slot_guard, slot_index))
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

  pub fn has_failed(&self) -> bool {
    let status = self.header.get_status_lock();
    status.has_failed()
  }

  pub fn finished_status(&self) -> BlockHasherStatus {
    let status = self.header.get_status_lock();
    status.finished_status()
  }
}
