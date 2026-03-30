use super::Signature;
use crate::hasher::{BlockHasher, BlockHasherStatus};
use crate::pool::{ContainerBackedPool, ContainerPool, Pool};
use crate::protos;

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
  pub fn bytes_to_fix(&self, container: &protos::Container) -> u64 {
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
fn check_file_integrity(
  entry_index: usize,
  src_pool: &mut impl ContainerBackedPool,
  hasher: &mut BlockHasher,
  progress_callback: impl FnMut(u64),
) -> Result<bool, String> {
  // Get the file size
  let container_file_size = src_pool.get_container_size(entry_index)?;
  let file_size = src_pool.get_size(entry_index)?;

  // If the length doesn't match, then this file is broken
  if file_size != Some(container_file_size) {
    return Ok(false);
  }

  let mut reader = src_pool.get_reader(entry_index)?;
  let status = hasher.hash_next_file(&mut reader, progress_callback)?;

  Ok(match status {
    BlockHasherStatus::Ok => true,
    BlockHasherStatus::HashMismatch { block_index: _ } => false,
  })
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
  ///   bytes read since the last one
  ///
  /// # Returns
  ///
  /// A [`IntegrityIssues`] struct that contains all files that failed verification.
  ///
  /// # Errors
  ///
  /// If there is an I/O failure while reading files or metadata.
  pub fn verify_files(
    &mut self,
    build_folder: &Path,
    mut progress_callback: impl FnMut(u64),
  ) -> Result<IntegrityIssues, String> {
    // This vector holds all the broken file indexes found in the build folder
    let mut broken_files: Vec<usize> = Vec::new();

    // Create the hasher that will verify the files' integrity
    let mut hasher = BlockHasher::new(&self.container_new, &mut self.block_hash_iter);

    // Load a pool from the build folder
    let mut src_pool = ContainerPool::open(&self.container_new, build_folder);

    // Loop over all the files in the source pool
    for entry_index in 0..src_pool.entry_count() {
      // Check if the file is intact
      let is_intact = check_file_integrity(
        entry_index,
        &mut src_pool,
        &mut hasher,
        &mut progress_callback,
      )?;

      // If not, add it to the broken files vector
      if !is_intact {
        broken_files.push(entry_index);
      }
    }

    Ok(IntegrityIssues {
      files: broken_files.into_boxed_slice(),
    })
  }
}
