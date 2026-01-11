use super::read::Signature;
use crate::container::{BLOCK_SIZE, ContainerItem};
use crate::hasher::{BlockHasher, BlockHasherError};
use crate::protos::tlc;

use std::io::Read;
use std::path::Path;

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

    // Create the hasher that will verify the files' integrity
    let mut hasher = BlockHasher::new(&mut self.block_hash_iter);

    // This buffer will hold some data for the hasher to verify
    // The length of the buffer doesn't need to be BLOCK_SIZE, any
    // value is valid
    let mut buffer = vec![0u8; BLOCK_SIZE as usize];

    // Loop over all the files in the signature container
    'file: for (file_index, container_file) in self.container_new.files.iter().enumerate() {
      // Reset the hasher to clean all the remaining data
      hasher.reset();

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
        hasher.skip_blocks(blocks_to_skip)?;
        progress_callback(blocks_to_skip);
        continue 'file;
      }

      // Wrapping the file inside a BufReader isn't needed because
      // the buffer is large
      let mut file = container_file.open_read(build_folder.to_owned())?;

      // Hash the whole file
      loop {
        let read_bytes = file
          .read(&mut *buffer)
          .map_err(|e| format!("Couldn't read file data into buffer!\n{e}"))?;

        if read_bytes == 0 {
          break;
        }

        // Update hasher and handle the error
        match hasher.update(&buffer[..read_bytes]) {
          Ok(hashed_blocks) => progress_callback(hashed_blocks),
          // If the error is due to the file being broken, set it as broken
          // and continue with the next one
          Err(BlockHasherError::HashMismatch { .. }) => {
            broken_files.push(file_index);
            let blocks_to_skip = container_file.block_count() - hasher.blocks_since_reset();
            hasher.skip_blocks(blocks_to_skip)?;
            progress_callback(blocks_to_skip);
            continue 'file;
          }
          // Else, return the error
          Err(e) => return Err(e.to_string()),
        };
      }

      // Hash the last block and handle the error
      match hasher.finalize_block() {
        // If the block was checked, callback!
        Ok(true) => progress_callback(1),
        // If not, don't
        Ok(false) => (),
        // If the error is due to the file being broken, set it as broken
        Err(BlockHasherError::HashMismatch { .. }) => {
          // All the blocks have been checked, don't skip
          broken_files.push(file_index);
          continue 'file;
        }
        // Else, return the error
        Err(e) => return Err(e.to_string()),
      }
    }

    Ok(IntegrityIssues {
      files: broken_files.into_boxed_slice(),
    })
  }
}
