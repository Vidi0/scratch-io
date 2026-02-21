use super::{FilesCache, FilesCacheStatus, OpStatus};
use crate::common::BLOCK_SIZE;
use crate::hasher::{BlockHasherStatus, FileBlockHasher};
use crate::patch::{OpIter, SyncHeader, SyncHeaderKind};
use crate::protos::tlc;

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum PatchFileStatus {
  Patched { written_bytes: u64 },
  Skipped { old_index: usize },
  Empty,
  Broken,
}

/// Set the length of a file, but only to shrink it
fn truncate_file(file: &mut File, new_len: u64) -> Result<(), String> {
  // Get the metadata
  let metadata = file
    .metadata()
    .map_err(|e| format!("Couldn't get new file metadata to truncate it!\n{e}"))?;

  // Return an error if the file is smaller than the new lenth
  if metadata.len() < new_len {
    return Err(format!("While loading a checkpoint, the size of the new file in disk can't be smaller than the checkpoint file length!
  File length: {}
  Checkpoint file length: {new_len}",
      metadata.len()
    ));
  }

  file
    .set_len(new_len)
    .map_err(|e| format!("Couldn't truncate file to load checkpoint!\n{e}"))
}

impl FileCheckpoint {
  /// Load a checkpoint
  ///
  /// If `old_file_seek_position` is provided, load it as a bsdiff checkpoint.
  /// Else, load it as an rsync one.
  pub fn load<K>(
    &self,
    written_bytes: &mut u64,
    op_index: &mut usize,
    old_file_seek_position: Option<&mut u64>,
    new_file: &mut File,
    op_iter: &mut OpIter<impl Read, K>,
    hasher: &mut Option<FileBlockHasher<impl Read>>,
  ) -> Result<(), String> {
    *op_index = match *self {
      FileCheckpoint::Rsync {
        written_bytes: c_bytes,
        op_index,
      } => {
        // The old file seek position must not exist for an rsync checkpoint
        let None = old_file_seek_position else {
          return Err("Can't load a bsdiff checkpoint for an rsync file patch!".to_string());
        };

        *written_bytes = c_bytes;
        op_index
      }
      FileCheckpoint::Bsdiff {
        written_bytes: c_bytes,
        old_file_seek_position: c_seek,
        op_index,
      } => {
        // The old file seek position must exist for a bsdiff checkpoint
        let Some(old_file_seek_position) = old_file_seek_position else {
          return Err("Can't load an rsync checkpoint for a bsdiff file patch!".to_string());
        };

        *written_bytes = c_bytes;
        *old_file_seek_position = c_seek;
        op_index
      }
    };

    // Add 1 to op_index
    // E.g: if the first operation was applied successfully (index 0),
    // then the current operation is the second one (index 1)
    *op_index += 1;

    // Truncate the new file to the correct size
    truncate_file(new_file, *written_bytes)?;

    // Skip hasher blocks
    if let Some(hasher) = hasher {
      hasher.skip_bytes(*written_bytes)?;
    }

    // Skip the patch operations
    op_iter.skip_operations(*op_index as u64)
  }
}

impl<R: Read> SyncHeader<'_, R> {
  /// Apply all the patch operations in the given header and
  /// write them into `new_file`
  #[allow(clippy::too_many_arguments)]
  pub fn patch_file(
    &mut self,
    // `new_file` is a clousure because the new file won't not be needed
    // if this patch represents a literal copy of an old file (the patch will be skipped)
    new_file: impl FnOnce() -> Result<File, String>,
    hasher: &mut Option<FileBlockHasher<impl Read>>,
    new_file_size: u64,
    old_files_cache: &mut FilesCache,
    container_old: &tlc::Container,
    patch_op_buffer: &mut Vec<u8>,
    checkpoint: Option<FileCheckpoint>,
    mut save_checkpoint: impl FnMut(FileCheckpoint) -> Result<(), String>,
    mut progress_callback: impl FnMut(u64),
  ) -> Result<PatchFileStatus, String> {
    let mut written_bytes: u64 = 0;

    // Save the index of the operation to be able to store it in the checkpoint
    let mut op_index: usize = 0;

    // WARNING: It is very important that, before any early Ok return
    // inside the self.kind match for both rsync and bsdiff iterators,
    // op_iter.drain() is called to ensure they aren't left in an invalid state

    match self.kind {
      SyncHeaderKind::Rsync { ref mut op_iter } => {
        // Rsync operations can be used to determine two special cases:
        //
        // 1. The new file is a literal copy of one in the old container
        // 2. The new file is empty
        //
        // For that reason, check if the *first* operation represents
        // one of these special cases.
        //
        // Skip the check if there is a checkpoint, because a checkpoint means this
        // patch operation represents actual changes in the file.
        let first = if checkpoint.is_some() {
          None
        } else {
          match op_iter.next() {
            Some(op) => {
              let first = op?;

              // If it represents an empty file, then return early
              if first.is_empty_file(new_file_size) {
                op_iter.drain()?;
                return Ok(PatchFileStatus::Empty);
              }

              // It it's a literal copy, return early, too
              if let Some(old_index) = first.is_literal_copy(new_file_size, container_old)? {
                progress_callback(new_file_size);

                op_iter.drain()?;
                return Ok(PatchFileStatus::Skipped { old_index });
              }

              // If it's not, return the SyncOp to be able to apply it later
              Some(first)
            }
            // If the first operation is None, something has gone wrong...
            // Even if the file is empty, it is represented with an empty Data message.
            None => {
              return Err(
                "Expected the first SyncOp for this file, but received None?".to_string(),
              );
            }
          }
        };

        // Now that we know that this file will have to be patched,
        // get the new file with the clousure
        let new_file = &mut new_file()?;

        // Load the checkpoint
        if let Some(c) = checkpoint {
          // If there is a checkpoint, then the first operation was not
          // obtained yet.
          assert!(first.is_none());

          // For that reason, it is possible to load the checkpoint normally:
          c.load(
            &mut written_bytes,
            &mut op_index,
            None,
            new_file,
            op_iter,
            hasher,
          )?;
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
        let iter = std::iter::once(first)
          .filter_map(|x| x.map(Ok))
          .chain(&mut *op_iter);

        // Get the index of the operation to be able to store it in the checkpoint
        for op in iter {
          let status = op?.apply(
            new_file,
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
          })?;

          op_index += 1;
        }
      }

      SyncHeaderKind::Bsdiff {
        target_index,
        ref mut op_iter,
      } => {
        // If the header kind is bsdiff, the file will have to be patched
        let new_file = &mut new_file()?;

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

        // Load the checkpoint
        if let Some(c) = checkpoint {
          c.load(
            &mut written_bytes,
            &mut op_index,
            Some(&mut old_file_seek_position),
            new_file,
            op_iter,
            hasher,
          )?;
        }

        // Rewind the old file to the start because the file might
        // have been in the cache and seeked before
        //
        // If there is a checkpoint, seek the file to the correct position
        old_file
          .seek(std::io::SeekFrom::Start(old_file_seek_position))
          .map_err(|e| format!("Couldn't seek old file to start!\n{e}"))?;

        // Finally, apply all the bsdiff operations
        for control in &mut *op_iter {
          let status = control?.apply(
            new_file,
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
          })?;

          op_index += 1;
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
