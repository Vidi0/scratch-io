mod bsdiff;
mod rsync;
mod staging;

use super::{Patch, SyncHeader, SyncHeaderKind};
use crate::hasher::{BlockHasher, BlockHasherError, BlockHasherStatus};
use crate::protos::tlc;
use crate::signature::BlockHashIter;

use std::fs;
use std::io::Read;
use std::path::Path;

const MAX_OPEN_FILES_PATCH: std::num::NonZeroUsize = std::num::NonZeroUsize::new(16).unwrap();

pub struct FilesCache<'a> {
  cache: lru::LruCache<usize, fs::File>,
  build_folder: &'a Path,
}

impl<'a> FilesCache<'a> {
  pub fn new(build_folder: &'a Path) -> Self {
    FilesCache {
      cache: lru::LruCache::new(MAX_OPEN_FILES_PATCH),
      build_folder,
    }
  }

  pub fn get_file(
    &mut self,
    index: usize,
    container: &tlc::Container,
  ) -> Result<&mut fs::File, String> {
    self.cache.try_get_or_insert_mut(index, || {
      container.open_file_read(index, self.build_folder.to_owned())
    })
  }
}

#[derive(Clone, Copy)]
#[must_use]
pub enum OpStatus {
  Ok,
  VerificationFailed,
}

fn verify_data(
  hasher: &mut Option<BlockHasher<'_, impl Read>>,
  data: &[u8],
) -> Result<OpStatus, BlockHasherError> {
  if let Some(hasher) = hasher
    && let BlockHasherStatus::HashMismatch { .. } = hasher.update(data)?
  {
    return Ok(OpStatus::VerificationFailed);
  }

  Ok(OpStatus::Ok)
}

impl Patch<'_> {
  /// Apply the patch operations to produce the new build.
  ///
  /// This creates all files, directories, and symlinks in `new_build_folder`,
  /// then applies each sync operation (rsync or bsdiff) using data from
  /// `old_build_folder`. Written data is hashed on the fly and verified against
  /// `hash_iter` (if provided). `progress_callback` is invoked with the number
  /// of processed bytes as the patch is applied.
  ///
  /// # Arguments
  ///
  /// * `old_build_folder` - The path to the old build folder
  ///
  /// * `new_build_folder` - The path to the new build folder
  ///
  /// * `hash_iter` - Iterator over expected block hashes used to verify the
  ///   integrity of the written files (optional)
  ///
  /// * `progress_callback` - A callback that is called with the number of
  ///   bytes processed since the last one
  ///
  /// # Errors
  ///
  /// If there is an I/O failure while reading files or metadata, or if hash
  /// verification of the generated files fails
  pub fn apply(
    &mut self,
    old_build_folder: &Path,
    new_build_folder: &Path,
    hash_iter: Option<&mut BlockHashIter<impl Read>>,
    mut progress_callback: impl FnMut(u64),
  ) -> Result<(), String> {
    // Create the new container folders, files and symlinks,
    // applying all the correct permissions
    self.container_new.create(new_build_folder)?;

    // Create a cache of open file descriptors for the old files
    // The key is the file_index of the old file provided by the patch
    // The value is the open file descriptor
    let mut old_files_cache = FilesCache::new(old_build_folder);

    // This buffer is used when applying bsdiff add operations
    // It is created here to avoid allocating and deallocating
    // the buffer on each add operation
    let mut add_buffer: Vec<u8> = Vec::new();

    // This buffer is used when applying rsync block_range operations
    // It is created here to avoid allocating and deallocating
    // the buffer on each block_range operation
    // It is only used when a hasher is provided
    let mut block_buffer: Vec<u8> = Vec::new();

    // If a hash_iter was provided, create a reusable hasher
    // instance to verify that the new game files are intact
    let mut hasher = hash_iter.map(|iter| BlockHasher::new(iter));

    // Patch all files in the iterator one by one
    while let Some(header) = self.sync_op_iter.next_header() {
      let mut header =
        header.map_err(|e| format!("Couldn't get next patch sync operation!\n{e}"))?;

      // Open the new file
      let new_container_file = self.container_new.get_file(header.file_index as usize)?;
      let mut new_file = new_container_file.open_write(new_build_folder.to_owned())?;

      // Write all the new data into the file
      header.patch_file(
        &mut new_file,
        &mut hasher,
        new_container_file.size as u64,
        &mut old_files_cache,
        &self.container_old,
        &mut add_buffer,
        &mut block_buffer,
        &mut progress_callback,
      )?;
    }

    Ok(())
  }
}
