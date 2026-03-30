use crate::common::BLOCK_SIZE;
use crate::signature::MD5_HASH_LENGTH;

use std::marker::PhantomData;
use std::sync::{Mutex, MutexGuard};

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
enum BufferStatus {
  WaitingForRefill,
  Refilling,
  WaitingForHash {
    block_index: usize,
    buffer_size: usize,
    expected_hash: [u8; MD5_HASH_LENGTH],
  },
  Hashing,
}

mod buffer_handle {
  #[derive(Clone, Copy, Debug)]
  pub struct Hash;
  #[derive(Clone, Copy, Debug)]
  pub struct Refill;
}

#[derive(Debug)]
pub struct BufferHandle<'a, K> {
  guard: MutexGuard<'a, [u8; BLOCK_SIZE]>,
  index: usize,
  // The length of the buffer, must be less or equal to BLOCK_SIZE
  len: usize,
  _kind: PhantomData<K>,
}

impl<K> BufferHandle<'_, K> {
  pub fn buffer(&self) -> &[u8] {
    &self.guard[..self.len]
  }

  pub fn buffer_mut(&mut self) -> &mut [u8] {
    &mut self.guard[..self.len]
  }
}

#[derive(Debug)]
pub struct FileBlock<'data> {
  pub block_index: usize,

  pub data: BufferHandle<'data, buffer_handle::Hash>,
  pub expected_hash: [u8; MD5_HASH_LENGTH],
}

#[derive(Debug)]
pub struct BlockBufferPool {
  status: Mutex<Vec<BufferStatus>>,
  buffers: Vec<Mutex<[u8; BLOCK_SIZE]>>,
}

impl BlockBufferPool {
  pub fn new(size: usize) -> Self {
    Self {
      status: Mutex::new(vec![BufferStatus::WaitingForRefill; size]),
      buffers: std::iter::repeat_with(|| Mutex::new([0u8; BLOCK_SIZE]))
        .take(size)
        .collect(),
    }
  }

  pub fn get_buffer_to_refill(
    &self,
    buffer_size: usize,
  ) -> Option<BufferHandle<'_, buffer_handle::Refill>> {
    assert!(buffer_size <= BLOCK_SIZE);

    let mut status = self.status.lock().unwrap();

    for (index, s) in status.iter_mut().enumerate() {
      if *s == BufferStatus::WaitingForRefill {
        // Getting the buffer won't lock because the status is WaitingForRefill,
        // so no other thread should have it.
        let guard = self.buffers[index]
          .try_lock()
          .expect("Buffer lock failed despite status indicating availability");

        // Set the status to Refilling so no other thread can try refilling it
        *s = BufferStatus::Refilling;

        return Some(BufferHandle {
          guard,
          index,
          len: buffer_size,
          _kind: PhantomData,
        });
      }
    }

    None
  }

  pub fn get_buffer_to_hash(&self) -> Option<FileBlock<'_>> {
    let mut status = self.status.lock().unwrap();

    for (index, s) in status.iter_mut().enumerate() {
      if let BufferStatus::WaitingForHash {
        block_index,
        buffer_size,
        expected_hash,
      } = *s
      {
        // Getting the buffer won't lock because the status is WaitingForHash,
        // so no other thread should have it.
        let guard = self.buffers[index]
          .try_lock()
          .expect("Buffer lock failed despite status indicating availability");

        // Set the status to Hashing so no other thread can try refilling it
        *s = BufferStatus::Hashing;

        return Some(FileBlock {
          block_index,
          data: BufferHandle {
            guard,
            index,
            len: buffer_size,
            _kind: PhantomData,
          },
          expected_hash,
        });
      }
    }

    None
  }

  /// Take ownership of the buffer handle in order to drop the guard and allow the
  /// buffer to be taken by other thread.
  pub fn save_refilled_buffer(
    &self,
    buffer: BufferHandle<'_, buffer_handle::Refill>,
    expected_hash: [u8; MD5_HASH_LENGTH],
    block_index: usize,
  ) {
    let index = buffer.index;
    let len = buffer.len;
    // release the MutexGuard FIRST before obtaining the status
    drop(buffer);

    let mut status = self.status.lock().unwrap();
    status[index] = BufferStatus::WaitingForHash {
      block_index,
      buffer_size: len,
      expected_hash,
    };
  }

  /// Take ownership of the buffer handle in order to drop the guard and allow the
  /// buffer to be taken by other thread.
  pub fn drop_hashed_buffer(&self, buffer: FileBlock<'_>) {
    let index = buffer.data.index;
    // release the MutexGuard FIRST before obtaining the status
    drop(buffer);

    let mut status = self.status.lock().unwrap();
    status[index] = BufferStatus::WaitingForRefill;
  }
}
