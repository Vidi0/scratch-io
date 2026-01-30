mod bsdiff;
mod rsync;

use super::{Patch, SyncHeader, SyncHeaderKind};
use crate::hasher::BlockHasher;
use crate::protos::*;
use crate::signature::BlockHashIter;

use std::fs;
use std::io::{Read, Seek, Write};
use std::path::Path;

const MAX_OPEN_FILES_PATCH: std::num::NonZeroUsize = std::num::NonZeroUsize::new(16).unwrap();

/// Apply all the patch operations in the given header and
/// write them into `writer`
fn patch_file<R: Read>(
  header: &mut SyncHeader<'_, R>,
  writer: &mut impl Write,
  old_files_cache: &mut lru::LruCache<usize, fs::File>,
  container_old: &tlc::Container,
  old_build_folder: &Path,
  add_buffer: &mut Vec<u8>,
  progress_callback: &mut impl FnMut(u64),
) -> Result<(), String> {
  match header.kind {
    // The current file will be updated using the Rsync method
    SyncHeaderKind::Rsync { ref mut op_iter } => {
      // Finally, apply all the rsync operations
      for op in op_iter {
        let op = op?;
        rsync::apply(
          &op,
          writer,
          old_files_cache,
          container_old,
          old_build_folder,
          progress_callback,
        )?;
      }
    }

    // The current file will be updated using the Bsdiff method
    SyncHeaderKind::Bsdiff {
      target_index,
      ref mut op_iter,
    } => {
      // Open the old file
      let old_file = old_files_cache.try_get_or_insert_mut(target_index as usize, || {
        container_old.open_file_read(target_index as usize, old_build_folder.to_owned())
      })?;

      // Rewind the old file to the start because the file might
      // have been in the cache and seeked before
      old_file
        .rewind()
        .map_err(|e| format!("Couldn't seek old file to start!\n{e}"))?;

      // Finally, apply all the bsdiff operations
      for control in op_iter {
        let control = control?;
        bsdiff::apply(control, writer, old_file, add_buffer, progress_callback)?;
      }
    }
  }

  Ok(())
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
    let mut old_files_cache: lru::LruCache<usize, fs::File> =
      lru::LruCache::new(MAX_OPEN_FILES_PATCH);

    // This buffer is used when applying bsdiff add operations
    // It is created here to avoid allocating and deallocating
    // the buffer on each add operation
    let mut add_buffer: Vec<u8> = Vec::new();

    // If a hash_iter was provided, create a reusable hasher
    // instance to verify that the new game files are intact
    let mut hasher = hash_iter.map(|iter| BlockHasher::new(iter));

    // Patch all files in the iterator one by one
    while let Some(header) = self.sync_op_iter.next_header() {
      let mut header =
        header.map_err(|e| format!("Couldn't get next patch sync operation!\n{e}"))?;

      // Open the new file
      let mut new_file = self
        .container_new
        .open_file_write(header.file_index as usize, new_build_folder.to_owned())?;

      // Write all the new data into the file
      match &mut hasher {
        // Wrap the new file in the hasher
        Some(h) => {
          h.reset();
          let mut hash_writer = h.wrap_writer(&mut new_file);

          patch_file(
            &mut header,
            &mut hash_writer,
            &mut old_files_cache,
            &self.container_old,
            old_build_folder,
            &mut add_buffer,
            &mut progress_callback,
          )?;
        }

        // Patch into the file directly without checking
        None => {
          patch_file(
            &mut header,
            &mut new_file,
            &mut old_files_cache,
            &self.container_old,
            old_build_folder,
            &mut add_buffer,
            &mut progress_callback,
          )?;
        }
      }

      // VERY IMPORTANT!
      // If the file doesn't finish with a full block, hash it anyways!
      if let Some(h) = &mut hasher {
        h.finalize_block()?;
      }
    }

    Ok(())
  }
}
