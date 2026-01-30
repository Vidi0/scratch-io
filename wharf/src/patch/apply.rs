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

// Whether the file to be patched was actually patched or was skipped
// because it was an exact copy of an old file
enum PatchFileStatus {
  Patched,
  Skipped,
}

impl<R: Read> SyncHeader<'_, R> {
  /// Apply all the patch operations in the given header and
  /// write them into `writer`
  #[allow(clippy::too_many_arguments)]
  fn patch_file(
    &mut self,
    writer: &mut impl Write,
    new_file_size: u64,
    old_files_cache: &mut lru::LruCache<usize, fs::File>,
    container_old: &tlc::Container,
    old_build_folder: &Path,
    add_buffer: &mut Vec<u8>,
    progress_callback: &mut impl FnMut(u64),
  ) -> Result<PatchFileStatus, String> {
    match self.kind {
      SyncHeaderKind::Rsync { ref mut op_iter } => {
        // Rsync operations can be used to determine literal copies of
        // files into the new container.
        //
        // For that reason, check if the *first* operation represents a literal copy
        let first = match op_iter.next() {
          Some(op) => op?,
          // If the first operation is None, something has gone wrong...
          // Even if the file is empty, it is represented with an empty Data message.
          None => {
            return Err("Expected the first SyncOp for this file, but received None?".to_string());
          }
        };

        if first.is_literal_copy(new_file_size, container_old)? {
          // IMPORTANT! To not break the iterator, call next() one more time
          // This way, the last message (HeyYouDidIt) for this file is read.
          // Its type will not be HeyYouDidIt, because when the iterators reachs
          // a message with this type, it returns None instead.
          match op_iter.next() {
            None => (),
            _ => {
              return Err(
                "After detecting a literal copy in this SyncOp, another one was returned?"
                  .to_string(),
              );
            }
          }

          progress_callback(new_file_size);
          return Ok(PatchFileStatus::Skipped);
        }

        // Finally, apply all the rsync operations
        // Don't forget the first one, which was obtained independently!
        for op in std::iter::once(Ok(first)).chain(op_iter) {
          let op = op?;
          op.apply(
            writer,
            old_files_cache,
            container_old,
            old_build_folder,
            progress_callback,
          )?;
        }
      }

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
          control.apply(writer, old_file, add_buffer, progress_callback)?;
        }
      }
    }

    Ok(PatchFileStatus::Patched)
  }
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
      let new_container_file = self.container_new.get_file(header.file_index as usize)?;
      let mut new_file = new_container_file.open_write(new_build_folder.to_owned())?;

      // Write all the new data into the file
      match &mut hasher {
        // Wrap the new file in the hasher
        Some(h) => {
          h.reset();
          let mut hash_writer = h.wrap_writer(&mut new_file);

          header.patch_file(
            &mut hash_writer,
            new_container_file.size as u64,
            &mut old_files_cache,
            &self.container_old,
            old_build_folder,
            &mut add_buffer,
            &mut progress_callback,
          )?;
        }

        // Patch into the file directly without checking
        None => {
          header.patch_file(
            &mut new_file,
            new_container_file.size as u64,
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
