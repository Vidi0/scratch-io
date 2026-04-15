mod buffer_pool;
mod errors;
mod internal_hasher;

pub use errors::{BlockHasherError, BlockHasherStatus};

use buffer_pool::{BufferPool, BufferPoolSession, HashBuffer};
use internal_hasher::InternalHasher;

use crate::common::{BLOCK_SIZE, block_count, block_size};
use crate::protos;
use crate::signature::{BlockHashIter, FileHashIter};

use std::io::Read;
use std::thread::{self, Builder};

/// Default number of hashers to use when the availble parallelism can't be determined
const DEFAULT_HASHERS_NUM: usize = 4;
/// Do hashing multithreaded for files with 4 or more blocks
const MIN_BLOCKS_FOR_MULTITHREADING: u64 = 4;
/// When verifying a file multithreaded, how many threads to use at minimum.
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
    let num_hashers = thread::available_parallelism()
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

/// Continuously hash blocks from the buffer pool until no more are available or a mismatch is found.
///
/// Must be run on a dedicated hasher thread. Blocks waiting for a filled buffer, hashes it,
/// then releases it back to the pool for refilling. On a mismatch, signals failure
/// via the pool and returns.
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

/// Read blocks from `reader` and feed them into the buffer pool for hasher threads to consume.
///
/// Must be run on a dedicated IO thread. For each block, reads the expected hash from `hash_iter`,
/// waits for a free buffer slot, fills it with file data, then releases it to the hashers.
/// Returns early without error if the pool signals that verification has finished
/// (e.g. a hasher found a mismatch).
///
/// # Errors
///
/// If the iterator returns an error or there is an I/O failure while reading the file.
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
    let buffer_len = block_size(block_index, file_size);

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

/// Hash all blocks of a file on the calling thread, verifying each against the signature.
///
/// Reads the file block by block from `reader`, hashing each block and comparing it
/// against the expected hash from `hash_iter`. Returns immediately on the first mismatch.
///
/// Intended for small files where spawning threads would cost more than the hashing itself.
///
/// # Returns
///
/// - [`BlockHasherStatus::Ok`] if all blocks match their expected hashes.
/// - [`BlockHasherStatus::HashMismatch`] if any block fails verification,
///   containing the index of the first mismatched block.
///
/// # Errors
///
/// If the iterator returns an error or there is an I/O failure while reading the file.
fn singlethreaded_hash(
  file_size: u64,
  hash_iter: &mut FileHashIter,
  reader: &mut impl Read,
  buffer: &mut [u8; BLOCK_SIZE],
  hasher: &mut InternalHasher,
  mut progress_callback: impl FnMut(u64) + Send,
) -> Result<BlockHasherStatus, BlockHasherError> {
  for (block_index, expected_hash) in hash_iter.enumerate() {
    // Read the expected hash from the signature
    let expected_hash = expected_hash
      .map(|hash| hash.strong_hash)
      .map_err(BlockHasherError::IterReturnedError)?;

    // Calculate how many bytes to read for this block
    let buffer_len = block_size(block_index, file_size);

    // Set the correct buffer size
    let buffer = &mut buffer[..buffer_len];

    // Read the file block into the buffer
    reader
      .read_exact(buffer)
      .map_err(BlockHasherError::ReaderFailed)?;

    // Hash the data and check the status
    let status = hasher.hash_data(block_index, &expected_hash, buffer);
    if let BlockHasherStatus::HashMismatch { .. } = status {
      return Ok(status);
    }

    progress_callback(buffer_len as u64);
  }

  Ok(BlockHasherStatus::Ok)
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

    // If there are only a few blocks, do hashing singlethreaded
    if file_blocks < MIN_BLOCKS_FOR_MULTITHREADING {
      // Reset the buffer pool for a singlethreaded session
      let buffer = self.buffer_pool.new_session_singlethreaded();

      let hasher = self
        .internal_hashers
        .get_mut(0)
        .expect("This BlockHasher must contain at least one internal_hasher!");

      return singlethreaded_hash(
        file_size,
        file_hash_iter,
        reader,
        buffer,
        hasher,
        progress_callback,
      );
    }

    // If there are many blocks, then hash in a multithreaded fashion

    // Reset the buffer pool
    let buffer_pool = &self.buffer_pool.new_session(file_blocks);

    thread::scope(|scope| {
      // Spawn the IO thread
      let io_handle = {
        Builder::new()
          .name("hasher IO".to_string())
          .spawn_scoped(scope, || -> Result<(), BlockHasherError> {
            io_thread(
              file_size,
              file_hash_iter,
              reader,
              buffer_pool,
              progress_callback,
            )
          })
          .unwrap()
      };

      // Spawn the hasher threads
      // If `file_blocks` is lower than the number of hashers, spawn only one hasher for each block
      for (index, hasher) in self
        .internal_hashers
        .iter_mut()
        .take(file_blocks as usize)
        .enumerate()
      {
        Builder::new()
          .name(format!("hasher {index}"))
          .spawn_scoped(scope, || hasher_thread(hasher, buffer_pool))
          .unwrap();
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
