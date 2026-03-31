use super::errors::BlockHasherStatus;

use std::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
  Running,
  Failed,
  Finished,
}

impl From<u8> for Status {
  fn from(value: u8) -> Self {
    match value {
      0 => Self::Running,
      1 => Self::Failed,
      2 => Self::Finished,
      _ => unreachable!(),
    }
  }
}

impl From<Status> for u8 {
  fn from(value: Status) -> Self {
    match value {
      Status::Running => 0,
      Status::Failed => 1,
      Status::Finished => 2,
    }
  }
}

pub struct VerificationStatus {
  status: AtomicU8,
  broken_block_index: AtomicUsize,
  // Countdown for finishing logic
  remaining_blocks: AtomicU64,
  // Only used for skip calculation on failure
  blocks_read: AtomicU64,
}

// Functions for hash iter skip calculation
impl VerificationStatus {
  /// Return the number of blocks read
  pub fn blocks_read(&self) -> u64 {
    self.blocks_read.load(Ordering::SeqCst)
  }

  /// Add one to the counter of hashed blocks
  pub fn add_one_read_block(&self) {
    self.blocks_read.fetch_add(1, Ordering::SeqCst);
  }
}

impl VerificationStatus {
  fn get_status(&self) -> Status {
    self.status.load(Ordering::SeqCst).into()
  }

  fn store_status(&self, status: Status) {
    self.status.store(status.into(), Ordering::SeqCst);
  }
}

impl VerificationStatus {
  pub fn new(total_blocks: u64) -> Self {
    let status = Self {
      status: AtomicU8::new(0),
      remaining_blocks: AtomicU64::new(total_blocks),
      broken_block_index: AtomicUsize::new(0),
      blocks_read: AtomicU64::new(0),
    };

    status.store_status(Status::Running);
    status
  }

  /// Return true if the verification has finished or if it has failed
  pub fn has_finished(&self) -> bool {
    matches!(self.get_status(), Status::Finished | Status::Failed)
  }

  /// Return true if the verification has failed
  pub fn has_failed(&self) -> bool {
    matches!(self.get_status(), Status::Failed)
  }

  /// This function sets the status as failed to indicate the IO and hasher
  /// threads to stop
  pub fn set_failed(&self, broken_block_index: usize) {
    self.store_status(Status::Failed);
    self
      .broken_block_index
      .store(broken_block_index, Ordering::SeqCst);
  }

  /// Add one to the counter of hashed blocks
  ///
  /// If this block is the last one to be hashed, set the status as finished
  pub fn add_one_hashed_block(&self) {
    // fetch_sub returns the old value
    let old_remaining_blocks = self.remaining_blocks.fetch_sub(1, Ordering::SeqCst);
    let new_remaining_blocks = old_remaining_blocks - 1;

    if new_remaining_blocks == 0 {
      // Only transition from Running to Finished, never overwrite Failed
      let _ = self.status.compare_exchange(
        Status::Running.into(),
        Status::Finished.into(),
        Ordering::SeqCst,
        Ordering::SeqCst,
      );
    }
  }

  /// This function must only be called after the hashing process has finished
  pub fn finished_status(&self) -> BlockHasherStatus {
    match self.get_status() {
      Status::Running => unreachable!(),
      Status::Finished => BlockHasherStatus::Ok,
      Status::Failed => {
        let block_index = self.broken_block_index.load(Ordering::SeqCst);
        BlockHasherStatus::HashMismatch { block_index }
      }
    }
  }
}
