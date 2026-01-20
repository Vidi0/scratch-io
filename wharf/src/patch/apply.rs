use super::read::{BsdiffOpIter, Patch, RsyncOpIter, SyncHeaderKind};
use crate::container::BLOCK_SIZE;
use crate::hasher::BlockHasher;
use crate::protos::*;
use crate::signature::read::BlockHashIter;

use std::fs;
use std::io::{self, Read, Seek, Write};
use std::path::Path;

const MAX_OPEN_FILES_PATCH: std::num::NonZeroUsize = std::num::NonZeroUsize::new(16).unwrap();

/// Copy blocks of bytes from `src` into `dst`
fn copy_range(
  src: &mut (impl Read + Seek),
  dst: &mut impl Write,
  block_index: u64,
  block_span: u64,
) -> Result<u64, String> {
  let start_pos = block_index * BLOCK_SIZE;
  let len = block_span * BLOCK_SIZE;

  src
    .seek(io::SeekFrom::Start(start_pos))
    .map_err(|e| format!("Couldn't seek into old file at pos: {start_pos}\n{e}"))?;

  let mut limited = src.take(len);

  io::copy(&mut limited, dst).map_err(|e| format!("Couldn't copy data from old file to new!\n{e}"))
}

/// Apply all `op_iter` rsync operations to regenerate the new file
/// into `writer` from the files in the old container
fn apply_rsync(
  op_iter: RsyncOpIter<impl Read>,
  writer: &mut impl Write,
  old_files_cache: &mut lru::LruCache<usize, fs::File>,
  old_container: &tlc::Container,
  old_build_folder: &Path,
  progress_callback: &mut impl FnMut(u64),
) -> Result<(), String> {
  // Apply all the sync operations
  for op in op_iter {
    let op = op?;

    match op.r#type() {
      // If the type is BlockRange, copy the range from the old file to the new one
      pwr::sync_op::Type::BlockRange => {
        // Open the old file
        let old_file = old_files_cache.try_get_or_insert_mut(op.file_index as usize, || {
          old_container.open_file_read(op.file_index as usize, old_build_folder.to_owned())
        })?;

        // Rewind isn't needed because the copy_range function already seeks
        // into the correct (not relative) position

        // Copy the specified range to the new file
        let written_bytes = copy_range(
          old_file,
          writer,
          op.block_index as u64,
          op.block_span as u64,
        )?;

        // Return the number of bytes copied into the new file
        progress_callback(written_bytes)
      }
      // If the type is Data, just copy the data from the patch to the new file
      pwr::sync_op::Type::Data => {
        writer
          .write_all(&op.data)
          .map_err(|e| format!("Couldn't copy data from patch to new file!\n{e}"))?;

        // Return the number of bytes written into the new file
        progress_callback(op.data.len() as u64)
      }
      // If the type is HeyYouDidIt, then the iterator would have returned None
      pwr::sync_op::Type::HeyYouDidIt => unreachable!(),
    }
  }

  Ok(())
}

/// Read a block from `src`, add corresponding bytes from `add`, and write the result to `dst`
fn add_bytes(
  src: &mut impl Read,
  dst: &mut impl Write,
  add: &[u8],
  add_buffer: &mut [u8],
) -> Result<(), String> {
  assert_eq!(add.len(), add_buffer.len());

  src
    .read_exact(add_buffer)
    .map_err(|e| format!("Couldn't read data from old file into buffer!\n{e}"))?;

  for i in 0..add.len() {
    add_buffer[i] += add[i];
  }

  dst
    .write_all(add_buffer)
    .map_err(|e| format!("Couldn't save buffer data into new file!\n{e}"))
}

/// Apply all `op_iter` bsdiff operations to regenerate the new file
/// into `writer` from `old_file`
fn apply_bsdiff(
  op_iter: BsdiffOpIter<impl Read>,
  writer: &mut impl Write,
  old_file: &mut fs::File,
  add_buffer: &mut Vec<u8>,
  progress_callback: &mut impl FnMut(u64),
) -> Result<(), String> {
  // Apply all the control operations
  for control in op_iter {
    let control = control?;

    // Control operations must be applied in order
    // First, add the diff bytes
    if !control.add.is_empty() {
      // Resize the add buffer to match the size of the current add bytes
      // The add operations are usually the same length, so allocation is almost never triggered
      // If the new add bytes are smaller than the buffer size, allocation will also be avoided
      add_buffer.resize(control.add.len(), 0);

      add_bytes(old_file, writer, &control.add, add_buffer)?;

      // Return the number of bytes added into the new file
      progress_callback(control.add.len() as u64);
    }

    // Then, copy the extra bytes
    if !control.copy.is_empty() {
      writer
        .write_all(&control.copy)
        .map_err(|e| format!("Couldn't copy data from patch to new file!\n{e}"))?;

      // Return the number of bytes copied into the new file
      progress_callback(control.copy.len() as u64);
    }

    // Lastly, seek into the correct position in the old file
    if control.seek != 0 {
      old_file.seek_relative(control.seek).map_err(|e| {
        format!(
          "Couldn't seek into old file at relative pos: {}\n{e}",
          control.seek
        )
      })?;
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
  /// `hash_iter`. `progress_callback` is invoked with the number of processed
  /// bytes as the patch is applied.
  ///
  /// # Arguments
  ///
  /// * `old_build_folder` - The path to the old build folder
  ///
  /// * `new_build_folder` - The path to the new build folder
  ///
  /// * `hash_iter` - Iterator over expected block hashes used to verify the
  ///   integrity of the written files
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
    hash_iter: &mut BlockHashIter<impl Read>,
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

    // Create a reusable hasher instance to verify that the new
    // game files are intact
    let mut hasher = BlockHasher::new(hash_iter);

    // Patch all files in the iterator one by one
    while let Some(header) = self.sync_op_iter.next_header() {
      let header = header.map_err(|e| format!("Couldn't get next patch sync operation!\n{e}"))?;

      // Open the new file
      let mut new_file = self
        .container_new
        .open_file_write(header.file_index as usize, new_build_folder.to_owned())?;

      // Wrap the new file in the hasher
      hasher.reset();
      let mut new_file_hasher = hasher.wrap_writer(&mut new_file);

      match header.kind {
        // The current file will be updated using the Rsync method
        SyncHeaderKind::Rsync { op_iter } => {
          // Finally, apply all the rsync operations
          apply_rsync(
            op_iter,
            &mut new_file_hasher,
            &mut old_files_cache,
            &self.container_old,
            old_build_folder,
            &mut progress_callback,
          )?;
        }

        // The current file will be updated using the Bsdiff method
        SyncHeaderKind::Bsdiff {
          target_index,
          op_iter,
        } => {
          // Open the old file
          let old_file = old_files_cache.try_get_or_insert_mut(target_index as usize, || {
            self
              .container_old
              .open_file_read(target_index as usize, old_build_folder.to_owned())
          })?;

          // Rewind the old file to the start because the file might
          // have been in the cache and seeked before
          old_file
            .rewind()
            .map_err(|e| format!("Couldn't seek old file to start!\n{e}"))?;

          // Finally, apply all the bsdiff operations
          apply_bsdiff(
            op_iter,
            &mut new_file_hasher,
            old_file,
            &mut add_buffer,
            &mut progress_callback,
          )?;
        }
      }

      // VERY IMPORTANT!
      // If the file doesn't finish with a full block, hash it anyways!
      hasher.finalize_block()?;
    }

    Ok(())
  }
}
