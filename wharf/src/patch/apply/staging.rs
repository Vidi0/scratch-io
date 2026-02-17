use super::{FilesCache, FilesCacheStatus, OpStatus, SyncHeader, SyncHeaderKind};
use crate::common::BLOCK_SIZE;
use crate::hasher::{BlockHasherStatus, FileBlockHasher};
use crate::protos::tlc;

use std::io::{Read, Seek, Write};

#[derive(Clone, Copy, Debug)]
#[must_use]
pub enum FileCheckpoint {
  Rsync {
    written_bytes: u64,
    op_index: usize,
  },
  Bsdiff {
    written_bytes: u64,
    old_file_seek_position: u64,
    op_index: usize,
  },
}

// Whether the file to be patched was actually patched or was skipped
// because it was an exact copy of an old file
#[derive(Clone, Copy, Debug)]
#[must_use]
pub enum PatchFileStatus {
  Patched { written_bytes: u64 },
  Skipped { old_index: u64 },
  Broken,
}

impl<R: Read> SyncHeader<'_, R> {
  /// Apply all the patch operations in the given header and
  /// write them into `writer`
  #[allow(clippy::too_many_arguments)]
  pub fn patch_file(
    &mut self,
    writer: &mut impl Write,
    hasher: &mut Option<FileBlockHasher<impl Read>>,
    new_file_size: u64,
    old_files_cache: &mut FilesCache,
    container_old: &tlc::Container,
    patch_op_buffer: &mut Vec<u8>,
    progress_callback: &mut impl FnMut(u64),
    //checkpoint: Option<FileCheckpoint>,
    save_checkpoint: &mut impl FnMut(FileCheckpoint),
  ) -> Result<PatchFileStatus, String> {
    let mut written_bytes: u64 = 0;

    // WARNING: It is very important that, before any early Ok return
    // inside the self.kind match for both rsync and bsdiff iterators,
    // op_iter.drain() is called to ensure they aren't left in an invalid state

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
          progress_callback(new_file_size);

          op_iter.drain()?;
          return Ok(PatchFileStatus::Skipped {
            old_index: first.file_index as u64,
          });
        }

        // Resize the block buffer
        // The size of the new buffer doesn't need to be BLOCK_SIZE,
        // but it makes sense to use it
        // Don't resize it if it's already large enough
        if patch_op_buffer.len() < BLOCK_SIZE as usize {
          patch_op_buffer.resize(BLOCK_SIZE as usize, 0);
        }

        // Finally, apply all the rsync operations
        // Don't forget the first one, which was obtained independently!
        // Get the index of the operation to be able to store it in the checkpoint
        for (op_index, op) in std::iter::once(Ok(first)).chain(&mut *op_iter).enumerate() {
          let status = op?.apply(
            writer,
            hasher,
            old_files_cache,
            container_old,
            patch_op_buffer,
          )?;

          match status {
            OpStatus::Ok { written_bytes: b } => {
              written_bytes += b;
              progress_callback(b);
            }
            OpStatus::Broken => {
              op_iter.drain()?;
              return Ok(PatchFileStatus::Broken);
            }
          }

          // Save a checkpoint after each successful patch operation
          save_checkpoint(FileCheckpoint::Rsync {
            written_bytes,
            op_index,
          })
        }
      }

      SyncHeaderKind::Bsdiff {
        target_index,
        ref mut op_iter,
      } => {
        // Open the old file
        let (old_file, old_file_disk_size) =
          match old_files_cache.get_file(target_index as usize, container_old)? {
            FilesCacheStatus::Ok {
              file,
              container_size: _,
              disk_size,
            } => (file, disk_size),
            FilesCacheStatus::NotFound => {
              op_iter.drain()?;
              return Ok(PatchFileStatus::Broken);
            }
          };

        // Store the old file seek position between apply calls
        let mut old_file_seek_position: u64 = 0;

        // Rewind the old file to the start because the file might
        // have been in the cache and seeked before
        old_file
          .seek(std::io::SeekFrom::Start(old_file_seek_position))
          .map_err(|e| format!("Couldn't seek old file to start!\n{e}"))?;

        // Finally, apply all the bsdiff operations
        // Get the index of the operation to be able to store it in the checkpoint
        for (op_index, control) in op_iter.enumerate() {
          let status = control?.apply(
            writer,
            hasher,
            old_file,
            &mut old_file_seek_position,
            old_file_disk_size,
            patch_op_buffer,
          )?;

          match status {
            OpStatus::Ok { written_bytes: b } => {
              written_bytes += b;
              progress_callback(b);
            }
            OpStatus::Broken => {
              op_iter.drain()?;
              return Ok(PatchFileStatus::Broken);
            }
          }

          // Save a checkpoint after each successful patch operation
          save_checkpoint(FileCheckpoint::Bsdiff {
            written_bytes,
            old_file_seek_position,
            op_index,
          })
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

    // If the patch is correct, the number of written bytes and the new
    // file size should match.
    //
    // If the number of written bytes is lower because the file couldn't be
    // patched or was skipped, then the function should have returned early.
    if written_bytes != new_file_size {
      return Err("After successfully patching a file, the number of written bytes does not equal the expected amount!".to_string());
    }

    Ok(PatchFileStatus::Patched { written_bytes })
  }
}
