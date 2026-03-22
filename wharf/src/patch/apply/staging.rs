use super::StagingFiles;
use crate::hasher::BlockHasher;
use crate::patch::SyncEntryIter;
use crate::patch::operations::apply::{FileCheckpoint, PatchFileStatus};
use crate::pool::{ContainerBackedPool, SeekablePool};

use serde::{Deserialize, Serialize};
use std::io::Read;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub struct StagingCheckpoint {
  /// A vector containing in order all the files that have been
  /// successfully patched
  patched_files: Vec<PatchFileStatus>,

  /// A checkpoint representing the file that is currently
  /// being patched
  current_file: Option<FileCheckpoint>,
}

impl StagingCheckpoint {
  /// Returns the index of the file that has to be patched
  /// or was being patched
  pub fn current_file_index(&self) -> u64 {
    self.patched_files.len() as u64
  }

  pub fn update_current_file_checkpoint(&mut self, checkpoint: FileCheckpoint) {
    self.current_file = Some(checkpoint)
  }

  pub fn push_status(&mut self, status: PatchFileStatus) {
    // Add the status to the vector of finished file patches
    self.patched_files.push(status);

    // Clear the current file checkpoint
    self.current_file = None;
  }

  /// Load the checkpoint
  pub fn load(
    &self,
    sync_op_iter: &mut SyncEntryIter<impl Read>,
    hasher: &mut Option<BlockHasher<impl Read>>,
  ) -> Result<(), String> {
    if self.current_file_index() == 0 {
      return Ok(());
    }

    // Skip to the correct sync header
    sync_op_iter.skip_entries(self.current_file_index())?;

    // Skip the hasher to the correct file
    if let Some(hasher) = hasher {
      hasher.skip_files(self.current_file_index() as usize)?;
    }

    Ok(())
  }
}

// Contains all the individual file patch status
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use]
pub struct ReconstructedFilesStatus {
  pub patched_files: Vec<PatchFileStatus>,
}

#[allow(clippy::too_many_arguments)]
pub fn reconstruct_modified_files(
  src_pool: &mut (impl SeekablePool + ContainerBackedPool),
  dst_pool: &mut impl ContainerBackedPool,
  sync_op_iter: &mut SyncEntryIter<impl Read>,
  staging_files: &StagingFiles,
  hasher: &mut Option<BlockHasher<impl Read>>,
  patch_op_buffer: &mut Vec<u8>,
  checkpoint: Option<StagingCheckpoint>,
  mut save_checkpoint: impl FnMut(&StagingCheckpoint) -> Result<(), String>,
  mut progress_callback: impl FnMut(u64),
) -> Result<ReconstructedFilesStatus, String> {
  // Get the default checkpoint (empty) if it doesn't exist
  // Because it is expensive to clone the checkpoint every time, it
  // is created here and reused for the whole function
  let mut checkpoint = checkpoint.unwrap_or_default();

  // Load the checkpoint
  checkpoint.load(sync_op_iter, hasher)?;

  // Important!
  // Send save checkpoint calls every time:
  //
  // 1. A new sync op operation is successfully applied
  // 2. Any file is successfully fully patched (or skipped, etc.)
  //
  // The caller should decide whether to actually store those checkpoints

  // Patch all files in the iterator one by one
  while let Some(header) = sync_op_iter.next_header() {
    let mut header = header.map_err(|e| format!("Couldn't get next patch sync operation!\n{e}"))?;

    // Get the new file index
    let file_index = header.file_index as usize;

    // Open the new file
    let new_file_size = dst_pool.get_container_size(file_index)?;
    let new_file = || staging_files.open_write(file_index);

    // Create a hasher for the current file
    let mut file_hasher = match hasher.as_mut() {
      Some(h) => Some(h.next_file_hasher()?),
      None => None,
    };

    // Write all the new data into the file
    let status = header.patch_file(
      new_file,
      &mut file_hasher,
      new_file_size,
      src_pool,
      patch_op_buffer,
      checkpoint.current_file,
      |file_c| {
        // If a sync op was successfully applied,
        // save a checkpoint with the new data
        checkpoint.update_current_file_checkpoint(file_c);
        save_checkpoint(&checkpoint)
      },
      &mut progress_callback,
    )?;

    // Update the checkpoint and save it
    checkpoint.push_status(status);
    save_checkpoint(&checkpoint)?;
  }

  Ok(ReconstructedFilesStatus {
    patched_files: checkpoint.patched_files,
  })
}
