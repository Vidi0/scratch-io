use super::errors::BlockHasherStatus;

use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
  Running,
  Finished,
  Failed,
}

impl From<u8> for Status {
  fn from(value: u8) -> Self {
    match value {
      0 => Self::Running,
      1 => Self::Finished,
      2 => Self::Failed,
      _ => unreachable!(),
    }
  }
}

impl From<Status> for u8 {
  fn from(value: Status) -> Self {
    match value {
      Status::Running => 0,
      Status::Finished => 1,
      Status::Failed => 2,
    }
  }
}

pub struct VerificationStatus {
  status: AtomicU8,
  broken_block_index: AtomicUsize,
}

impl VerificationStatus {
  pub fn new() -> Self {
    let status = Self {
      status: AtomicU8::new(0),
      broken_block_index: AtomicUsize::new(0),
    };

    status.store_status(Status::Running);
    status
  }

  fn get_status(&self) -> Status {
    self.status.load(Ordering::SeqCst).into()
  }

  fn store_status(&self, status: Status) {
    self.status.store(status.into(), Ordering::SeqCst);
  }

  /// Return true if the verification has finished or if it has failed
  pub fn has_finished(&self) -> bool {
    matches!(self.get_status(), Status::Finished | Status::Failed)
  }

  /// Return true if the verification has failed
  pub fn has_failed(&self) -> bool {
    matches!(self.get_status(), Status::Failed)
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

  pub fn set_finished(&self) {
    self.store_status(Status::Finished);
  }

  pub fn set_failed(&self, broken_block_index: usize) {
    self.store_status(Status::Failed);
    self
      .broken_block_index
      .store(broken_block_index, Ordering::SeqCst);
  }
}
