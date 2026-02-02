use super::{OpStatus, SyncHeader, SyncHeaderKind};
use crate::hasher::{BlockHasher, BlockHasherStatus};
use crate::protos::*;

use std::fs;
use std::io::{Read, Seek, Write};
use std::path::Path;

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub enum FileCheckpoint {
  Rsync {
    new_file_size: u64,
    op_index: u64,
  },
  Bsdiff {
    new_file_size: u64,
    op_index: u64,
    old_file_seek_position: u64,
  },
}

// Whether the file to be patched was actually patched or was skipped
// because it was an exact copy of an old file
//#[must_use]
pub enum PatchFileStatus {
  Patched,
  Skipped,
  Broken,
}

impl<R: Read> SyncHeader<'_, R> {
  /// Apply all the patch operations in the given header and
  /// write them into `writer`
  #[allow(clippy::too_many_arguments)]
  pub fn patch_file(
    &mut self,
    writer: &mut impl Write,
    hasher: &mut Option<BlockHasher<'_, impl Read>>,
    new_file_size: u64,
    old_files_cache: &mut lru::LruCache<usize, fs::File>,
    container_old: &tlc::Container,
    old_build_folder: &Path,
    add_buffer: &mut Vec<u8>,
    progress_callback: &mut impl FnMut(u64),
    //load_checkpoint: Option<FileCheckpoint>,
    //checkpoint: &mut impl FnMut(FileCheckpoint),
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
          let status = op?.apply(
            writer,
            hasher,
            old_files_cache,
            container_old,
            old_build_folder,
            progress_callback,
          )?;

          if let OpStatus::VerificationFailed = status {
            return Ok(PatchFileStatus::Broken);
          }
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
          let status = control?.apply(writer, hasher, old_file, add_buffer, progress_callback)?;

          if let OpStatus::VerificationFailed = status {
            return Ok(PatchFileStatus::Broken);
          }
        }
      }
    }

    // VERY IMPORTANT!
    // If the file doesn't finish with a full block, hash it anyways!
    if let Some(h) = hasher {
      let status = h.finalize_block()?;
      if let BlockHasherStatus::HashMismatch { .. } = status {
        return Ok(PatchFileStatus::Broken);
      }
    }

    Ok(PatchFileStatus::Patched)
  }
}
