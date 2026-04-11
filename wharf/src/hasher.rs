mod buffer_pool;
mod errors;
mod internal_hasher;

pub use errors::{BlockHasherError, BlockHasherStatus};

use buffer_pool::{BufferPool, BufferPoolSession, HashBuffer};
use internal_hasher::InternalHasher;

use crate::common::{BLOCK_SIZE, block_count};
use crate::protos;
use crate::signature::{BlockHashIter, FileHashIter};

use std::io::Read;

/// Default number of hashers to use when the availble parallelism can't be determined
const DEFAULT_HASHERS_NUM: usize = 4;
const MIN_THREADS: usize = 2;

pub struct BlockHasher<'cont, 'hash_iter, 'reader> {
  container: &'cont protos::Container,
  entry_index: usize,

  hash_iter: &'hash_iter mut BlockHashIter<'reader>,
  buffer_pool: BufferPool,

  internal_hashers: Vec<InternalHasher>,
}

impl<'cont, 'hash_iter, 'reader> BlockHasher<'cont, 'hash_iter, 'reader> {
  pub fn new(
    container: &'cont protos::Container,
    hash_iter: &'hash_iter mut BlockHashIter<'reader>,
  ) -> Self {
    let num_hashers = std::thread::available_parallelism()
      .map(|n| n.get())
      .unwrap_or(DEFAULT_HASHERS_NUM)
      .max(MIN_THREADS);

    assert!(num_hashers > 0);

    Self {
      container,
      entry_index: 0,

      hash_iter,
      // Create twice as many block buffers as internal hashers to avoid wasting
      // time waiting for the hashers to finish before obtaining more file data.
      buffer_pool: BufferPool::new(2 * num_hashers),

      internal_hashers: vec![InternalHasher::new(); num_hashers],
    }
  }
}

impl BlockHasher<'_, '_, '_> {
  /// Return the size of the next file in the container
  fn current_file_size(&self) -> Result<u64, BlockHasherError> {
    self
      .container
      .files
      .get(self.entry_index)
      .map(|f| f.size as u64)
      .ok_or(BlockHasherError::RunOutOfFiles {
        file_index: self.entry_index,
      })
  }
}

impl BlockHasher<'_, '_, '_> {
  fn skip_file(&mut self) -> Result<(), BlockHasherError> {
    let file_size = self.current_file_size()?;
    self.entry_index += 1;

    self
      .hash_iter
      .skip_blocks(block_count(file_size))
      .map_err(BlockHasherError::IterReturnedError)
  }

  fn skip_files(&mut self, file_count: usize) -> Result<(), BlockHasherError> {
    for _ in 0..file_count {
      self.skip_file()?;
    }

    Ok(())
  }
}

fn hasher_thread(hasher: &mut InternalHasher, buffer_pool: &BufferPoolSession) {
  loop {
    let Some(buffer) = buffer_pool.get_buffer_to_hash() else {
      return;
    };

    let status = hasher.hash_block(&buffer);

    // Leave the block buffer available to be filled by the IO thread again
    buffer_pool.release_hashed_buffer(buffer);

    if let BlockHasherStatus::HashMismatch { block_index } = status {
      buffer_pool.set_failed(block_index);
      return;
    }
  }
}

fn io_thread(
  file_size: u64,
  hash_iter: &mut FileHashIter,
  reader: &mut impl Read,
  buffer_pool: &BufferPoolSession,
  mut progress_callback: impl FnMut(u64) + Send,
) -> Result<(), BlockHasherError> {
  for (block_index, expected_hash) in hash_iter.enumerate() {
    // Read the expected hash from the signature
    let expected_hash = expected_hash
      .map(|hash| hash.strong_hash)
      .map_err(BlockHasherError::IterReturnedError)?;

    // Calculate how many bytes to read for this block
    let buffer_len = {
      let bytes_remaining = file_size as usize - (block_index * BLOCK_SIZE);
      BLOCK_SIZE.min(bytes_remaining)
    };

    // Get the block buffer
    let Some(mut block_buffer) = buffer_pool.get_buffer_to_refill(block_index, buffer_len) else {
      return Ok(());
    };

    // Store the expected hash into the buffer
    block_buffer.set_expected_hash(expected_hash);

    // Read the file block into the buffer
    reader
      .read_exact(block_buffer.buffer_mut())
      .map_err(BlockHasherError::ReaderFailed)?;

    // Share the block buffer with the hasher threads
    buffer_pool.release_refilled_buffer(block_buffer);

    progress_callback(buffer_len as u64);
  }

  Ok(())
}

impl BlockHasher<'_, '_, '_> {
  /// Hash the next file and verify its integrity against the signature
  ///
  /// Reads the file block by block from `reader`, hashing each block and
  /// comparing it against the expected hash from the signature iterator.
  /// If a hash mismatch is found, the remaining blocks are skipped in the
  /// signature iterator to keep it in sync, and [`BlockHasherStatus::HashMismatch`]
  /// is returned with the index of the first broken block.
  ///
  /// # Returns
  ///
  /// - [`BlockHasherStatus::Ok`] if all blocks match their expected hashes.
  /// - [`BlockHasherStatus::HashMismatch`] if any block fails verification,
  ///   containing the index of the first broken block.
  ///
  /// # Errors
  ///
  /// If the signature iterator runs out of hashes before all blocks are read,
  /// the iterator returns an error, or there is an I/O failure while reading
  /// the file.
  ///
  /// # Panics
  ///
  /// If `file_index` is lower or equal to the one provided in the last call
  /// to this method.
  pub fn hash_next_file<F>(
    &mut self,
    reader: &mut F,
    file_index: usize,
    progress_callback: impl FnMut(u64) + Send,
  ) -> Result<BlockHasherStatus, BlockHasherError>
  where
    F: Read + Send,
  {
    assert!(file_index >= self.entry_index);

    // Skip the files that hasn't been hashed in order to advance the hasher
    // and avoid breaking the hasher iterator
    self.skip_files(file_index - self.entry_index)?;

    // Get the next file size and reader
    let file_size = self.current_file_size()?;
    let file_blocks = block_count(file_size);

    // Before incrementing the entry index, the file index
    // must be equal to the entry index in self
    assert_eq!(file_index, self.entry_index);
    self.entry_index += 1;

    // Get the hash block iterator for the current file
    let file_hash_iter = &mut self
      .hash_iter
      .next_file(file_blocks)
      .map_err(BlockHasherError::CouldNotObtainIter)?;

    // Reset the buffer pool
    let buffer_pool = &self.buffer_pool.new_session(file_blocks);

    std::thread::scope(|scope| {
      // Spawn the IO thread
      let io_handle = {
        scope.spawn(|| -> Result<(), BlockHasherError> {
          io_thread(
            file_size,
            file_hash_iter,
            reader,
            buffer_pool,
            progress_callback,
          )
        })
      };

      // Spawn the hasher threads
      // If `file_blocks` is lower than the number of hashers, spawn only one hasher for each block
      for hasher in self.internal_hashers.iter_mut().take(file_blocks as usize) {
        scope.spawn(|| hasher_thread(hasher, buffer_pool));
      }

      // Check the IO thread result
      // If it errored, signal the hashers to stop and propagate the error
      if let Err(e) = io_handle.join().unwrap() {
        buffer_pool.set_failed(0);
        return Err(e);
      }

      Ok(())
    })?;

    Ok(buffer_pool.finished_status())
  }
}
