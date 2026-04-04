use super::BlockHasherStatus;

use parking_lot::{Condvar, Mutex, MutexGuard};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotStatus {
  WaitingForRefill,
  Refilling,
  WaitingForHash,
  Hashing,
}

enum VerificationStatus {
  Running { remaining_blocks: u64 },
  Failed { broken_block_index: usize },
  Finished,
}

pub struct PoolStatus {
  status: VerificationStatus,
  slots: Vec<SlotStatus>,
}

impl PoolStatus {
  /// Before using the [`PoolStatus`], it must be reset to the correct number
  /// of blocks via [`PoolStatus::reset`].
  pub fn new(pool_size: usize) -> Self {
    Self {
      status: VerificationStatus::Finished,
      slots: vec![SlotStatus::WaitingForRefill; pool_size],
    }
  }

  /// Set up the [`PoolStatus`] for verifying a new file, after the last verification
  /// has finished.
  pub fn reset(&mut self, total_blocks: u64) {
    self.status = VerificationStatus::Running {
      remaining_blocks: total_blocks,
    };

    // Set every slot in `self.slots` to WaitingForRefill
    for slot in &mut self.slots {
      *slot = SlotStatus::WaitingForRefill;
    }
  }

  pub fn has_finished(&self) -> bool {
    matches!(
      self.status,
      VerificationStatus::Finished | VerificationStatus::Failed { .. }
    )
  }

  pub fn finished_status(&self) -> BlockHasherStatus {
    match self.status {
      VerificationStatus::Running { .. } => unreachable!(),
      VerificationStatus::Finished => BlockHasherStatus::Ok,
      VerificationStatus::Failed {
        broken_block_index: i,
      } => BlockHasherStatus::HashMismatch { block_index: i },
    }
  }

  /// Find a slot with status `expected_status` and replace it with `new_status`,
  /// returning the index of the slot that has been changed.
  ///
  /// Returns None if the verification has finished.
  pub fn find_slot(
    mut guard: MutexGuard<'_, PoolStatus>,
    condvar: &Condvar,
    expected_status: SlotStatus,
    new_status: SlotStatus,
  ) -> Option<usize> {
    loop {
      // If the verification has finished, don't give away more slots!
      if guard.has_finished() {
        return None;
      }

      // Check for an available slot
      for (index, slot) in guard.slots.iter_mut().enumerate() {
        if *slot == expected_status {
          *slot = new_status;
          return Some(index);
        }
      }

      // Sleep until one slot is available
      condvar.wait(&mut guard);
    }
  }

  /// Release a slot found by [`PoolStatus::find_slot`] and set its new status
  /// to `new_status`, given its index `slot_index`.
  pub fn release_slot(&mut self, slot_index: usize, new_status: SlotStatus) {
    self.slots[slot_index] = new_status;
  }

  /// Returns `true` if hashing has finished
  pub fn add_one_hashed_block(&mut self) -> bool {
    if let VerificationStatus::Running { remaining_blocks } = &mut self.status {
      *remaining_blocks -= 1;

      if *remaining_blocks == 0 {
        self.status = VerificationStatus::Finished;
        return true;
      }
    }

    false
  }

  pub fn set_failed(&mut self, broken_block_index: usize) {
    self.status = VerificationStatus::Failed { broken_block_index }
  }
}

pub struct Header {
  status: Mutex<PoolStatus>,

  pub refill_ready: Condvar,
  pub hash_ready: Condvar,
}

impl Header {
  pub fn new(pool_size: usize) -> Self {
    Self {
      status: Mutex::new(PoolStatus::new(pool_size)),
      refill_ready: Condvar::new(),
      hash_ready: Condvar::new(),
    }
  }

  pub fn reset(&mut self, total_blocks: u64) {
    let mut status = self
      .status
      .try_lock()
      .expect("could not get status lock in header when doing a reset");

    status.reset(total_blocks);
  }

  /// Get a [`MutexGuard`] holding the header status
  pub fn get_status_lock(&self) -> MutexGuard<'_, PoolStatus> {
    self.status.lock()
  }
}
