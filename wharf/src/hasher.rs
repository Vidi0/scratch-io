mod errors;

use crate::common::{BLOCK_SIZE, block_count};
use crate::protos::tlc;
use crate::signature::BlockHashIter;
pub use errors::{BlockHasherError, BlockHasherStatus};

use md5::digest::{OutputSizeUser, generic_array::GenericArray, typenum::Unsigned};
use md5::{Digest, Md5};
use std::fs::File;
use std::io::{Read, Seek};

type Md5HashSize = <md5::Md5 as OutputSizeUser>::OutputSize;
pub const MD5_HASH_LENGTH: usize = Md5HashSize::USIZE;

pub struct BlockHasher<'cont, 'hash, R> {
  container: &'cont tlc::Container,
  entry_index: usize,

  hash_iter: &'hash mut BlockHashIter<R>,
  hasher: Md5,
  hash_buffer: GenericArray<u8, Md5HashSize>,

  last_file_remaining_blocks: u64,
}

impl<'cont, 'hash, R> BlockHasher<'cont, 'hash, R> {
  pub fn new(container: &'cont tlc::Container, hash_iter: &'hash mut BlockHashIter<R>) -> Self {
    Self {
      container,
      entry_index: 0,

      hash_iter,
      hasher: Md5::new(),
      hash_buffer: GenericArray::<u8, Md5HashSize>::default(),

      last_file_remaining_blocks: 0,
    }
  }
}

impl<'cont, 'hash, R> BlockHasher<'cont, 'hash, R>
where
  R: Read,
{
  pub fn next_file_hasher(&mut self) -> Result<FileBlockHasher<'_, 'cont, 'hash, R>, String> {
    // Reset the hasher, allowing it to hash another file
    self.hasher.reset();

    // Skip the blocks of the previous file that have not been hashed
    // to advance the iterator into the correct position
    self
      .hash_iter
      .skip_blocks(self.last_file_remaining_blocks)?;

    // Get the next file size
    let file_size = self
      .container
      .files
      .get(self.entry_index)
      .ok_or_else(|| {
        format!(
          "Couldn't get next file hasher because the container has run out of files!
Index: {}",
          self.entry_index
        )
      })?
      .size as u64;

    // Set up the internal counter to the right values
    self.last_file_remaining_blocks = block_count(file_size);
    self.entry_index += 1;

    Ok(FileBlockHasher {
      block_hasher: self,
      first_block: true,
      written_bytes: 0,
    })
  }

  pub fn skip_files(&mut self, file_sizes: impl Iterator<Item = u64>) -> Result<(), String> {
    for size in file_sizes {
      // Skip all the blocks
      let blocks = block_count(size);
      self.hash_iter.skip_blocks(blocks)?;
    }

    // Reset the hasher variables
    self.last_file_remaining_blocks = 0;

    Ok(())
  }
}

pub struct FileBlockHasher<'hasher, 'cont, 'hash, R> {
  block_hasher: &'hasher mut BlockHasher<'cont, 'hash, R>,

  first_block: bool,
  written_bytes: usize,
}

impl<R: Read> FileBlockHasher<'_, '_, '_, R> {
  /// Update the hahser with new data
  pub fn update(&mut self, mut buf: &[u8]) -> Result<BlockHasherStatus, BlockHasherError> {
    while !buf.is_empty() {
      // If all the expected blocks have been hashed, return an error
      if self.block_hasher.last_file_remaining_blocks == 0 {
        return Err(BlockHasherError::AllBlocksHashed);
      }

      // Get the next buffer slice
      let block_remaining = BLOCK_SIZE as usize - self.written_bytes;
      let to_take = block_remaining.min(buf.len());

      // Update the hasher
      self.block_hasher.hasher.update(&buf[..to_take]);

      // Update internal counters
      self.written_bytes += to_take;
      buf = &buf[to_take..];

      if self.written_bytes == BLOCK_SIZE as usize {
        // Chunk completed
        let status = self.finalize_block()?;
        if let BlockHasherStatus::HashMismatch { expected, found } = status {
          return Ok(BlockHasherStatus::HashMismatch { expected, found });
        }
      }
    }

    Ok(BlockHasherStatus::Ok)
  }

  /// Finalize the current data in the hasher and check the current block
  ///
  /// Don't hash the block if it's empty AND it isn't the first one
  pub fn finalize_block(&mut self) -> Result<BlockHasherStatus, BlockHasherError> {
    // Skip hashing if the current block is empty
    // However, wharf saves an empty hash for an empty file,
    // so ensure this is not the first block before skipping
    if self.written_bytes == 0 && !self.first_block {
      return Ok(BlockHasherStatus::Ok);
    }

    // Reset hasher variables
    self.first_block = false;
    self.written_bytes = 0;

    // Get the next hash from the iterator
    let next_hash = self
      .block_hasher
      .hash_iter
      .next()
      .ok_or(BlockHasherError::MissingHashFromIter)?
      .map_err(BlockHasherError::IterReturnedError)?;

    // After getting the hash from the iterator, decrease the
    // remaining blocks counter
    self.block_hasher.last_file_remaining_blocks -= 1;

    // Calculate the hash
    self
      .block_hasher
      .hasher
      .finalize_into_reset(&mut self.block_hasher.hash_buffer);

    // Compare the hashes
    if *self.block_hasher.hash_buffer != *next_hash.strong_hash {
      return Ok(BlockHasherStatus::HashMismatch {
        expected: next_hash.strong_hash,
        found: self.block_hasher.hash_buffer.into(),
      });
    }

    Ok(BlockHasherStatus::Ok)
  }

  /// This function MUST be called when this [`FileBlockHasher`]
  /// has just been created
  ///
  /// This function will move the file seek!
  pub fn skip_bytes(&mut self, bytes: u64, file: &mut File) -> Result<(), String> {
    assert!(self.first_block);
    assert_eq!(self.written_bytes, 0);

    // A number of whole blocks will be skipped, and then
    // the last block will be ignored
    let whole_blocks_to_skip = bytes / BLOCK_SIZE;
    let last_block_bytes = bytes % BLOCK_SIZE;

    // Ensure the number of blocks to skip is correct
    // Use div_ceil because there must be space left for the data
    // that doesn't complete a full block at the end
    if bytes.div_ceil(BLOCK_SIZE) > self.block_hasher.last_file_remaining_blocks {
      return Err(format!(
        "{} blocks are needed from hasher, only {} are remaining!",
        bytes.div_ceil(BLOCK_SIZE),
        self.block_hasher.last_file_remaining_blocks
      ))?;
    }

    // Skip the blocks
    if whole_blocks_to_skip > 0 {
      self
        .block_hasher
        .hash_iter
        .skip_blocks(whole_blocks_to_skip)?;

      self.block_hasher.last_file_remaining_blocks -= whole_blocks_to_skip;
      self.first_block = false;
    }

    // Hash the last block data that's currently in the file
    if last_block_bytes > 0 {
      let mut last_bytes_buf = vec![0u8; last_block_bytes as usize];

      file
        .seek(std::io::SeekFrom::Start(bytes - last_block_bytes))
        .map_err(|e| format!("Couldn't seek file to skip hasher bytes!\n{e}"))?;

      file.read_exact(&mut last_bytes_buf).map_err(|e| {
        format!("Couldn't read the exact bytes into the bufer to skip hasher bytes!\n{e}")
      })?;

      // The result can be ignored because the block won't be finalized
      // (last_block_bytes is less than BLOCK_SIZE) and there are blocks
      // remaining to hash (is was checked above)
      let _ = self.update(&last_bytes_buf);
    }

    Ok(())
  }
}
