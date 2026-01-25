use super::Signature;
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

/// Check if the provided file is intact or broken
///
/// # Returns
///
/// If the file is intact, returns `true`
fn check_file_integrity<R: Read>(
  file_path: &Path,
  container_file: &tlc::File,
  hasher: &mut BlockHasher<'_, R>,
  buffer: &mut [u8],
  progress_callback: &mut impl FnMut(u64),
) -> Result<bool, String> {
  // Reset the hasher to clean all the leftover data
  hasher.reset();

  // Get the file size
  let file_size = container_file.size as u64;

  // Check if the file exists and the length matches
  if match file_path.metadata() {
    // If the length doesn't match, then this file is broken
    Ok(m) => m.len() != file_size,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
    Err(e) => return Err(format!("Couldn't get file metadata!\n{e}")),
  } {
    let blocks_to_skip = container_file.block_count();
    hasher.skip_blocks(blocks_to_skip)?;
    progress_callback(file_size);
    return Ok(false);
  }

  // Wrapping the file inside a BufReader isn't needed because
  // the buffer is already large
  let mut file = container_file.open_read_from_path(file_path)?;

  // The total number of bytes that have been read
  let mut total_read_bytes: u64 = 0;

  // Hash the whole file
  loop {
    let read_bytes = file
      .read(buffer)
      .map_err(|e| format!("Couldn't read file data into buffer!\n{e}"))?;

    if read_bytes == 0 {
      break;
    }

    // Callback with the number of bytes read
    progress_callback(read_bytes as u64);
    total_read_bytes += read_bytes as u64;

    // Update hasher and handle the error
    match hasher.update(&buffer[..read_bytes]) {
      Ok(()) => (),
      // If the error is due to the file being broken, set it as broken
      // and continue with the next one
      Err(BlockHasherError::HashMismatch { .. }) => {
        let blocks_to_skip = container_file.block_count() - hasher.blocks_since_reset();
        hasher.skip_blocks(blocks_to_skip)?;
        // Callback the number of bytes that have not been called back before
        progress_callback(file_size - total_read_bytes);
        return Ok(false);
      }
      // Else, return the error
      Err(e) => return Err(e.to_string()),
    };
  }

  // Hash the last block and handle the error
  match hasher.finalize_block() {
    Ok(()) => (),
    // If the error is due to the file being broken, set it as broken
    Err(BlockHasherError::HashMismatch { .. }) => {
      // All the blocks have been checked, don't skip
      return Ok(false);
    }
    // Else, return the error
    Err(e) => return Err(e.to_string()),
  }

  Ok(true)
}

impl Signature<'_> {
  /// Verify the integrity of all files in the container
  ///
  /// This function iterates over every file in the container and checks if
  /// it exists and is not corrupted.
  ///
  /// Files that are missing, have mismatched sizes, or contain corrupted
  /// blocks are collected and returned in the [`IntegrityIssues`] structure.
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
  ///   bytes processed since the last one
  ///
  /// # Returns
  ///
  /// A [`IntegrityIssues`] struct that contains all files that failed verification.
  ///
  /// # Errors
  ///
  /// If there is an I/O failure while reading files or metadata.
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
    for (file_index, container_file) in self.container_new.files.iter().enumerate() {
      // Get file path
      let file_path = container_file.get_path(build_folder.to_owned())?;

      // Check if the file is intact
      let is_intact = check_file_integrity(
        &file_path,
        container_file,
        &mut hasher,
        &mut buffer,
        &mut progress_callback,
      )?;

      // If not, add it to the broken files vector
      if !is_intact {
        broken_files.push(file_index);
      }
    }

    Ok(IntegrityIssues {
      files: broken_files.into_boxed_slice(),
    })
  }
}
