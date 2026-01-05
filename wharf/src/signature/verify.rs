use super::read::Signature;
use crate::container::{BLOCK_SIZE, ContainerItem};
use crate::protos::tlc;

use md5::{Digest, Md5};
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrityIssues<'a> {
  // This vector must NOT contain duplicates!
  pub files: Vec<&'a tlc::File>,
}

impl IntegrityIssues<'_> {
  #[must_use]
  pub fn are_files_intact(&self) -> bool {
    self.files.is_empty()
  }

  #[must_use]
  pub fn bytes_to_fix(&self) -> u64 {
    self
      .files
      .iter()
      .fold(0, |acc, file| acc + file.size as u64)
  }
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
  ) -> Result<IntegrityIssues<'_>, String> {
    // This vector holds all the integrity issues found in the build folder
    let mut integrity_issues = IntegrityIssues { files: Vec::new() };

    // This buffer will hold the current block that is being hashed
    let mut buffer = vec![0u8; BLOCK_SIZE as usize];

    // Create a MD5 hasher
    let mut hasher = Md5::new();

    // Loop over all the files in the signature container
    'file: for container_file in &self.container_new.files {
      // Get file path
      let file_path = container_file.get_path(build_folder.to_owned())?;

      let file_size = container_file.size as u64;

      // Check if the file exists
      let exists = file_path
        .try_exists()
        .map_err(|e| format!("Couldn't check if the file exists!\n{e}"))?;

      if !exists {
        integrity_issues.files.push(container_file);
        let skipped_blocks = self.block_hash_iter.skip_file(file_size, 0)?;
        progress_callback(skipped_blocks);
        continue 'file;
      }

      // Check if the file length matches
      let metadata = file_path
        .metadata()
        .map_err(|e| format!("Couldn't get file metadata!\n{e}"))?;

      if metadata.len() != file_size {
        integrity_issues.files.push(container_file);
        let skipped_blocks = self.block_hash_iter.skip_file(file_size, 0)?;
        progress_callback(skipped_blocks);
        continue 'file;
      }

      // Wrapping the file inside a BufReader isn't needed because
      // BLOCK_SIZE is already large
      let mut file = container_file.open_read(&file_path)?;

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

        // Hash the current block
        hasher.update(buf);
        let hash = hasher.finalize_reset();

        // Get the expected hash from the signature
        let signature_hash = self.block_hash_iter.next().ok_or_else(|| {
          "Expected a block hash message in the signature, but EOF was encountered!".to_string()
        })??;

        // One new hash has been read, callback!
        progress_callback(1);

        // Compare the hashes
        if *signature_hash.strong_hash != *hash {
          integrity_issues.files.push(container_file);
          let skipped_blocks = self.block_hash_iter.skip_file(file_size, block_index + 1)?;
          progress_callback(skipped_blocks);
          continue 'file;
        }

        // If the file has been fully read, proceed to the next one
        if block_index * BLOCK_SIZE + current_block_size == file_size {
          continue 'file;
        }

        block_index += 1;
      }
    }

    Ok(integrity_issues)
  }
}
