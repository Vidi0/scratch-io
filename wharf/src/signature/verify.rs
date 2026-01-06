use super::read::Signature;
use crate::container::{BLOCK_SIZE, ContainerItem};
use crate::protos::tlc;

use md5::digest::{OutputSizeUser, generic_array::GenericArray, typenum::Unsigned};
use md5::{Digest, Md5};
use std::io::Read;
use std::path::Path;

const MD5_HASH_LENGTH: usize = <Md5 as OutputSizeUser>::OutputSize::USIZE;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrityIssues {
  /// Contains the indexes of the broken files in the new container
  ///
  /// This slice must NOT contain duplicates!
  pub files: Box<[usize]>,
}

impl IntegrityIssues {
  #[must_use]
  pub fn are_files_intact(&self) -> bool {
    self.files.is_empty()
  }

  #[must_use]
  pub fn bytes_to_fix(&self, container: &tlc::Container) -> u64 {
    self
      .files
      .iter()
      .fold(0, |acc, &i| acc + container.files[i].size as u64)
  }
}

/// Hash `buffer` and compare the hash with the expected one
///
/// Returns true if the hashes are equal, false otherwise
fn hash_block(
  hasher: &mut Md5,
  hash_buffer: &mut [u8; MD5_HASH_LENGTH],
  expected_hash: &[u8],
  buffer: &[u8],
) -> bool {
  // Hash the current block
  hasher.update(buffer);
  hasher.finalize_into_reset(GenericArray::from_mut_slice(hash_buffer));

  *hash_buffer == *expected_hash
}

impl Signature<'_> {
  /// Verify the integrity of all files in the container
  ///
  /// This function iterates over every file in the container and checks if
  /// it exists and is not corrupted.
  ///
  /// For each verified block, `progress_callback` is called with the number
  /// of blocks processed since the last callback. Files that are missing,
  /// have mismatched sizes, or contain corrupted blocks are collected and
  /// returned in the [`IntegrityIssues`] structure.
  ///
  /// This function does NOT check if the folders and symlinks in the container
  /// exist on the disk or if the modes (permissions) of the files, folders
  /// and symlinks are correct.
  ///
  /// # Arguments
  ///
  /// * `build_folder` - The path to the build folder
  ///
  /// * `progress_callback` - A callback that is called with the number of
  ///   blocks processed since the last one
  ///
  /// # Returns
  ///
  /// A [`IntegrityIssues`] struct that contains all files that failed verification.
  ///
  /// # Errors
  ///
  /// If there is an I/O failure while reading files or metadata.
  ///
  pub fn verify_files(
    &'_ mut self,
    build_folder: &Path,
    mut progress_callback: impl FnMut(u64),
  ) -> Result<IntegrityIssues, String> {
    // This vector holds all the broken file indexes found in the build folder
    let mut broken_files: Vec<usize> = Vec::new();

    // This buffer will hold the current block that is being hashed
    let mut buffer = vec![0u8; BLOCK_SIZE as usize];

    // Create a MD5 hasher
    let mut hasher = Md5::new();
    let mut hash_buffer = [0u8; MD5_HASH_LENGTH];

    // Loop over all the files in the signature container
    'file: for (file_index, container_file) in self.container_new.files.iter().enumerate() {
      // Get file path
      let file_path = container_file.get_path(build_folder.to_owned())?;

      // Get the file size
      let file_size = container_file.size as u64;

      // Check if the file exists and the length matches
      if match file_path.metadata() {
        // If the length doesn't match, then this file is broken
        Ok(m) => m.len() != file_size,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
        Err(e) => return Err(format!("Couldn't get file metadata!\n{e}")),
      } {
        broken_files.push(file_index);
        let blocks_to_skip = container_file.block_count();
        self.block_hash_iter.skip_blocks(blocks_to_skip)?;
        progress_callback(blocks_to_skip);
        continue 'file;
      }

      // Wrapping the file inside a BufReader isn't needed because
      // BLOCK_SIZE is already large
      let mut file = container_file.open_read(build_folder.to_owned())?;

      // For each block in the file, compare its hash with the one provided in the signature
      let mut block_index: u64 = 0;

      loop {
        // The size of the current block is BLOCK_SIZE,
        // unless there are less remaining bytes on the file
        let current_block_size = BLOCK_SIZE.min(file_size - block_index * BLOCK_SIZE);

        // Read the current block
        let buf = &mut buffer[..current_block_size as usize];
        file
          .read_exact(buf)
          .map_err(|e| format!("Couldn't read file data into buffer!\n{e}"))?;

        // Get the expected hash from the signature
        let signature_hash = self.block_hash_iter.next().ok_or_else(|| {
          "Expected a block hash message in the signature, but EOF was encountered!".to_string()
        })??;

        let equal = hash_block(
          &mut hasher,
          &mut hash_buffer,
          &signature_hash.strong_hash,
          buf,
        );

        // One new hash has been read, callback!
        progress_callback(1);

        // Compare the hashes
        if !equal {
          broken_files.push(file_index);
          let blocks_to_skip = container_file.block_count() - (block_index + 1);
          self.block_hash_iter.skip_blocks(blocks_to_skip)?;
          progress_callback(blocks_to_skip);
          continue 'file;
        }

        // If the file has been fully read, proceed to the next one
        if block_index * BLOCK_SIZE + current_block_size == file_size {
          continue 'file;
        }

        block_index += 1;
      }
    }

    Ok(IntegrityIssues {
      files: broken_files.into_boxed_slice(),
    })
  }
}
