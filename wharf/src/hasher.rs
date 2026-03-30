mod block_buffer;
mod errors;
mod internal_hasher;
mod verification_status;

pub use errors::{BlockHasherError, BlockHasherStatus};

use block_buffer::BlockBufferPool;
use internal_hasher::InternalHasher;
use verification_status::VerificationStatus;

use crate::common::{BLOCK_SIZE, block_count};
use crate::protos;
use crate::signature::BlockHashIter;

use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};

/// Default number of hashers to use when the availble parallelism can't be determined
const DEFAULT_HASHERS_NUM: usize = 4;
const MIN_THREADS: usize = 2;
const MIN_BLOCKS_FOR_MULTITHREADING: u64 = 1;

pub struct BlockHasher<'cont, 'hash_iter, 'reader> {
  container: &'cont protos::Container,
  entry_index: usize,

  hash_iter: &'hash_iter mut BlockHashIter<'reader>,
  block_buffers: BlockBufferPool,

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

    Self {
      container,
      entry_index: 0,

      hash_iter,
      // Create twice as many block buffers as internal hashers to avoid wasting
      // time waiting for the hashers to finish before obtaining more file data.
      block_buffers: BlockBufferPool::new(2 * num_hashers),

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
  pub fn skip_file(&mut self) -> Result<(), String> {
    let file_size = self.current_file_size()?;
    self.entry_index += 1;

    self.hash_iter.skip_blocks(block_count(file_size))
  }

  pub fn skip_files(&mut self, file_count: usize) -> Result<(), String> {
    for _ in 0..file_count {
      self.skip_file()?;
    }

    Ok(())
  }
}

#[expect(clippy::too_many_arguments)]
fn io_thread(
  verification_status: &VerificationStatus,
  file_size: u64,
  file_blocks: u64,
  read_blocks: &mut AtomicU64,
  hash_iter: &mut &mut BlockHashIter,
  reader: &mut impl Read,
  buffer_pool: &BlockBufferPool,
  mut progress_callback: impl FnMut(u64) + Send,
) -> Result<(), BlockHasherError> {
  for block_index in 0..file_blocks as usize {
    if verification_status.has_finished() {
      return Ok(());
    }

    // Read the expected hash from the signature
    let expected_hash = hash_iter
      .next()
      .ok_or(BlockHasherError::MissingHashFromIter)?
      .map(|hash| hash.strong_hash)
      .map_err(BlockHasherError::IterReturnedError)?;

    // Add 1 to the read blocks counter after reading the hash from the iterator
    read_blocks.fetch_add(1, Ordering::SeqCst);

    // Calculate how many bytes to read for this block
    let buffer_len = {
      let bytes_remaining = file_size as usize - (block_index * BLOCK_SIZE);
      BLOCK_SIZE.min(bytes_remaining)
    };

    // Get the block buffer
    let mut block_buffer = {
      // It is safe to loop here because the hasher threads will have to finish
      // hashing at some point.
      loop {
        if let Some(b) = buffer_pool.get_buffer_to_refill(block_index, buffer_len) {
          break b;
        } else {
          // Check if verification has failed to avoid deadlocks
          if verification_status.has_finished() {
            return Ok(());
          }

          std::hint::spin_loop();
        }
      }
    };

    // Store the expected hash into the buffer
    block_buffer.set_expected_hash(expected_hash);

    // Read the file block into the buffer
    reader
      .read_exact(block_buffer.buffer_mut())
      .map_err(BlockHasherError::ReaderFailed)?;

    // Share the block buffer with the hasher threads
    buffer_pool.drop_buffer(block_buffer);

    progress_callback(buffer_len as u64);
  }

  verification_status.set_finished();

  Ok(())
}

fn hasher_thread(
  verification_status: &VerificationStatus,
  hasher: &mut InternalHasher,
  buffer_pool: &BlockBufferPool,
) -> Result<(), BlockHasherError> {
  loop {
    if verification_status.has_finished() {
      return Ok(());
    }

    let buffer = loop {
      if let Some(b) = buffer_pool.get_buffer_to_hash() {
        break b;
      } else {
        // Check if verification has failed to avoid deadlocks
        if verification_status.has_finished() {
          return Ok(());
        }

        std::hint::spin_loop();
      }
    };

    let status = hasher.hash_block(&buffer);

    // Leave the block buffer available to be filled by the IO thread again
    buffer_pool.drop_buffer(buffer);

    if let BlockHasherStatus::HashMismatch { block_index } = status {
      verification_status.set_failed(block_index);
      return Ok(());
    }
  }
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
  pub fn hash_next_file<F>(
    &mut self,
    reader: &mut F,
    progress_callback: impl FnMut(u64) + Send,
  ) -> Result<BlockHasherStatus, BlockHasherError>
  where
    F: Read + Send,
  {
    // Get the next file size and reader
    let file_size = self.current_file_size()?;
    let file_blocks = block_count(file_size);

    self.entry_index += 1;

    let mut read_blocks = AtomicU64::new(0);
    let verification_status = VerificationStatus::new();
    let buffer_pool = &self.block_buffers;

    std::thread::scope(|scope| {
      // Spawn the IO thread
      {
        let verification_status = &verification_status;
        let read_blocks = &mut read_blocks;
        let hash_iter = &mut self.hash_iter;

        scope.spawn(move || -> Result<(), BlockHasherError> {
          io_thread(
            verification_status,
            file_size,
            file_blocks,
            read_blocks,
            hash_iter,
            reader,
            buffer_pool,
            progress_callback,
          )
        });
      }

      // Spawn the hasher threads
      for hasher in &mut self.internal_hashers {
        let verification_status = &verification_status;

        scope.spawn(move || -> Result<(), BlockHasherError> {
          hasher_thread(verification_status, hasher, buffer_pool)
        });

        // If there are only few blocks to hash, spawn only one hasher thread
        if file_blocks <= MIN_BLOCKS_FOR_MULTITHREADING {
          break;
        }
      }
    });

    if verification_status.has_failed() {
      self
        .hash_iter
        .skip_blocks(file_blocks - read_blocks.load(Ordering::SeqCst))
        .map_err(BlockHasherError::IterReturnedError)?;
    }

    Ok(verification_status.finished_status())
  }
}
