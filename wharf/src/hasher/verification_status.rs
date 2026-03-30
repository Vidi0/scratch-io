use super::errors::BlockHasherStatus;

use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

enum Status {
  Running,
  Finished,
  Failed,
}

impl Status {
  fn load(value: &AtomicU8) -> Self {
    let status = value.load(Ordering::SeqCst);

    match status {
      0 => Self::Running,
      1 => Self::Finished,
      2 => Self::Failed,
      _ => unreachable!(),
    }
  }

  fn store(&self, value: &AtomicU8) {
    let status = match self {
      Self::Running => 0,
      Self::Finished => 1,
      Self::Failed => 2,
    };

    value.store(status, Ordering::SeqCst);
  }
}

pub struct VerificationStatus {
  status: AtomicU8,
  broken_block_index: AtomicUsize,
}

impl VerificationStatus {
  pub fn new() -> Self {
    let status = AtomicU8::new(0);
    Status::Running.store(&status);

    Self {
      status,
      broken_block_index: AtomicUsize::new(0),
    }
  }

  /// Return true if the verification has finished or if it has failed
  pub fn has_finished(&self) -> bool {
    let status = Status::load(&self.status);
    matches!(status, Status::Finished | Status::Failed)
  }

  pub fn has_failed(&self) -> bool {
    let status = Status::load(&self.status);
    matches!(status, Status::Failed)
  }

  pub fn status(&self) -> BlockHasherStatus {
    let status = Status::load(&self.status);

    match status {
      Status::Running => unreachable!(),
      Status::Finished => BlockHasherStatus::Ok,
      Status::Failed => {
        let block_index = self.broken_block_index.load(Ordering::SeqCst);
        BlockHasherStatus::HashMismatch { block_index }
      }
    }
  }

  pub fn set_finished(&self) {
    Status::Finished.store(&self.status);
  }

  pub fn set_failed(&self, broken_block_index: usize) {
    Status::Failed.store(&self.status);
    self
      .broken_block_index
      .store(broken_block_index, Ordering::SeqCst);
  }
}
