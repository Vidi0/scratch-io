use super::read::{BsdiffOpIter, Patch, RsyncOpIter, SyncHeaderKind};
use crate::container::BLOCK_SIZE;
use crate::protos::*;

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
) -> Result<(), String> {
  let start_pos = block_index * BLOCK_SIZE;
  let len = block_span * BLOCK_SIZE;

  src
    .seek(io::SeekFrom::Start(start_pos))
    .map_err(|e| format!("Couldn't seek into old file at pos: {start_pos}\n{e}"))?;

  let mut limited = src.take(len);

  io::copy(&mut limited, dst)
    .map(|_| ())
    .map_err(|e| format!("Couldn't copy data from old file to new!\n{e}"))
}

/// Apply all `op_iter` rsync operations to regenerate `new_file`
/// from the files in the old container
fn apply_rsync(
  op_iter: RsyncOpIter<impl io::BufRead>,
  new_file: &mut fs::File,
  old_files_cache: &mut lru::LruCache<usize, fs::File>,
  old_container: &tlc::Container,
  old_build_folder: &Path,
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
        copy_range(
          old_file,
          new_file,
          op.block_index as u64,
          op.block_span as u64,
        )?;
      }
      // If the type is Data, just copy the data from the patch to the new file
      pwr::sync_op::Type::Data => {
        new_file
          .write_all(&op.data)
          .map_err(|e| format!("Couldn't copy data from patch to new file!\n{e}"))?;
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

/// Apply all `op_iter` bsdiff operations to regenerate `new_file` from `old_file`
fn apply_bsdiff(
  op_iter: BsdiffOpIter<impl Read>,
  new_file: &mut fs::File,
  old_file: &mut fs::File,
  add_buffer: &mut Vec<u8>,
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

      add_bytes(old_file, new_file, &control.add, add_buffer)?;
    }

    // Then, copy the extra bytes
    if !control.copy.is_empty() {
      new_file
        .write_all(&control.copy)
        .map_err(|e| format!("Couldn't copy data from patch to new file!\n{e}"))?;
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
  pub fn apply(
    &mut self,
    old_build_folder: &Path,
    new_build_folder: &Path,
    mut progress_callback: impl FnMut(),
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

    // Patch all files in the iterator one by one
    while let Some(header) = self.sync_op_iter.next_header() {
      let header = header.map_err(|e| format!("Couldn't get next patch sync operation!\n{e}"))?;

      // Open the new file
      let mut new_file = self
        .container_new
        .open_file_write(header.file_index as usize, new_build_folder.to_owned())?;

      match header.kind {
        // The current file will be updated using the Rsync method
        SyncHeaderKind::Rsync { op_iter } => {
          // Finally, apply all the rsync operations
          apply_rsync(
            op_iter,
            &mut new_file,
            &mut old_files_cache,
            &self.container_old,
            old_build_folder,
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
          apply_bsdiff(op_iter, &mut new_file, old_file, &mut add_buffer)?;
        }
      }

      // One new file has been patched, callback!
      progress_callback();
    }

    Ok(())
  }
}
