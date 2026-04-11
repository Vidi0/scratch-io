use super::OpStatus;
use crate::common::BLOCK_SIZE;
use crate::patch::operations::skip::{BsdiffIterator, RsyncIterator};
use crate::pool::{ContainerBackedPool, SeekablePool};

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Seek;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum FileCheckpointKind {
  Rsync,
  Bsdiff { old_file_seek_position: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub struct FileCheckpoint {
  written_bytes: u64,
  op_index: usize,
  kind: FileCheckpointKind,
}

impl FileCheckpoint {
  pub fn rsync(written_bytes: u64, op_index: usize) -> Self {
    Self {
      written_bytes,
      op_index,
      kind: FileCheckpointKind::Rsync,
    }
  }

  pub fn bsdiff(written_bytes: u64, op_index: usize, old_file_seek_position: u64) -> Self {
    Self {
      written_bytes,
      op_index,
      kind: FileCheckpointKind::Bsdiff {
        old_file_seek_position,
      },
    }
  }
}

/// The result of patching a file
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum PatchFileStatus {
  /// The file was successfully patched and `written_bytes` bytes were written
  Patched { written_bytes: u64 },

  /// The file was an exact copy of the old file at `old_index` and was skipped
  LiteralCopy { old_index: usize },

  /// The file is empty and no data was written
  Empty,

  /// The file could not be patched (the old container has missing data)
  Broken,

  /// The file contains the wrong data, so its contents cannot be trusted
  VerificationFailed,
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
  fn load_common(
    &self,
    written_bytes: &mut u64,
    op_index: &mut usize,
    new_file: &mut File,
  ) -> Result<(), String> {
    // Add 1 to op_index
    // E.g: if the first operation was applied successfully (index 0),
    // then the current operation is the second one (index 1)
    *op_index = self.op_index + 1;
    *written_bytes = self.written_bytes;

    // Truncate the new file to the correct size
    truncate_file(new_file, *written_bytes)
  }

  /// Load a checkpoint for an rsync patch
  pub fn load_rsync(
    &self,
    written_bytes: &mut u64,
    op_index: &mut usize,
    new_file: &mut File,
    op_iter: &mut RsyncIterator,
  ) -> Result<(), String> {
    let FileCheckpointKind::Rsync = self.kind else {
      return Err("Can't load a bsdiff checkpoint for an rsync file patch!".to_string());
    };

    self.load_common(written_bytes, op_index, new_file)?;

    // Skip the patch operations
    op_iter.skip_operations(*op_index as u64)
  }

  /// Load a checkpoint for a bsdiff patch
  pub fn load_bsdiff(
    &self,
    written_bytes: &mut u64,
    op_index: &mut usize,
    old_file_seek_position: &mut u64,
    new_file: &mut File,
    op_iter: &mut BsdiffIterator,
  ) -> Result<(), String> {
    let FileCheckpointKind::Bsdiff {
      old_file_seek_position: checkpoint_seek,
    } = self.kind
    else {
      return Err("Can't load an rsync checkpoint for a bsdiff file patch!".to_string());
    };

    self.load_common(written_bytes, op_index, new_file)?;
    *old_file_seek_position = checkpoint_seek;

    // Skip the patch operations
    op_iter.skip_operations(*op_index as u64)
  }
}

#[expect(clippy::too_many_arguments)]
pub fn patch_rsync(
  op_iter: &mut RsyncIterator,
  new_file: &mut File,
  new_file_size: u64,
  src_pool: &mut (impl SeekablePool + ContainerBackedPool),
  patch_op_buffer: &mut Vec<u8>,
  checkpoint: Option<FileCheckpoint>,
  mut save_checkpoint: impl FnMut(FileCheckpoint) -> Result<(), String>,
  mut progress_callback: impl FnMut(u64) + Send,
) -> Result<PatchFileStatus, String> {
  let mut op_index: usize = 0;
  let mut written_bytes: u64 = 0;

  // Load the checkpoint
  if let Some(c) = checkpoint {
    c.load_rsync(&mut written_bytes, &mut op_index, new_file, op_iter)?;
  }

  // Resize the block buffer
  // The size of the new buffer doesn't need to be BLOCK_SIZE,
  // but it makes sense to use it
  // Don't resize it if it's already large enough
  if patch_op_buffer.len() < BLOCK_SIZE {
    patch_op_buffer.resize(BLOCK_SIZE, 0);
  }

  for op in op_iter {
    let status = op?.apply(new_file, src_pool, patch_op_buffer)?;

    match status {
      OpStatus::Broken => return Ok(PatchFileStatus::Broken),
      OpStatus::Ok { written_bytes: b } => {
        written_bytes += b;
        progress_callback(b);
      }
    }

    // Save a checkpoint after each successful patch operation
    save_checkpoint(FileCheckpoint::rsync(written_bytes, op_index))?;

    op_index += 1;
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

#[expect(clippy::too_many_arguments)]
pub fn patch_bsdiff(
  op_iter: &mut BsdiffIterator,
  target_index: usize,
  new_file: &mut File,
  new_file_size: u64,
  src_pool: &mut (impl SeekablePool + ContainerBackedPool),
  patch_op_buffer: &mut Vec<u8>,
  checkpoint: Option<FileCheckpoint>,
  mut save_checkpoint: impl FnMut(FileCheckpoint) -> Result<(), String>,
  mut progress_callback: impl FnMut(u64) + Send,
) -> Result<PatchFileStatus, String> {
  let mut op_index: usize = 0;
  let mut written_bytes: u64 = 0;
  let mut old_file_seek_position: u64 = 0;

  // Load the checkpoint
  if let Some(c) = checkpoint {
    c.load_bsdiff(
      &mut written_bytes,
      &mut op_index,
      &mut old_file_seek_position,
      new_file,
      op_iter,
    )?;
  }

  // Open the old file
  let Some(old_file_disk_size) = src_pool.get_size(target_index)? else {
    return Ok(PatchFileStatus::Broken);
  };

  let mut old_file = src_pool.get_seek_reader(target_index)?;

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
      &mut old_file,
      &mut old_file_seek_position,
      old_file_disk_size,
      patch_op_buffer,
    )?;

    match status {
      OpStatus::Broken => return Ok(PatchFileStatus::Broken),
      OpStatus::Ok { written_bytes: b } => {
        written_bytes += b;
        progress_callback(b);
      }
    }

    // Save a checkpoint after each successful patch operation
    save_checkpoint(FileCheckpoint::bsdiff(
      written_bytes,
      op_index,
      old_file_seek_position,
    ))?;

    op_index += 1;
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
