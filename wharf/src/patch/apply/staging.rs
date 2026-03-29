use crate::hasher::{BlockHasher, BlockHasherStatus};
use crate::patch::SyncEntryIter;
use crate::patch::operations::{
  apply::{self, FileCheckpoint, PatchFileStatus},
  skip::SkipStatus,
};
use crate::pool::{ContainerBackedPool, Pool, SeekablePool, StagingPool, WritablePool};

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

pub fn reconstruct_modified_files(
  src_pool: &mut (impl SeekablePool + ContainerBackedPool),
  staging_pool: &mut StagingPool,
  dst_pool: &mut impl ContainerBackedPool,
  sync_op_iter: &mut SyncEntryIter<impl Read>,
  hasher: &mut Option<BlockHasher<impl Read>>,
  patch_op_buffer: &mut Vec<u8>,
  mut progress_callback: impl FnMut(u64),
) -> Result<ReconstructedFilesStatus, String> {
  // Deserialize the last checkpoint stored in the staging folder
  // Get the default checkpoint (empty) if it doesn't exist
  // Because it is expensive to clone the checkpoint every time, it
  // is created here and reused for the whole function
  let mut checkpoint = staging_pool
    .load_checkpoint::<StagingCheckpoint>()?
    .unwrap_or_default();

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
    let header = header.map_err(|e| format!("Couldn't get next patch sync operation!\n{e}"))?;

    // Get the new file index and size
    let file_index = header.file_index;
    let new_file_size = dst_pool.get_container_size(file_index)?;

    // Before patching, check if the file really needs patching
    let mut status = match header.check_skip(new_file_size, src_pool)? {
      SkipStatus::Empty => PatchFileStatus::Empty,
      SkipStatus::LiteralCopy { old_index } => PatchFileStatus::LiteralCopy { old_index },
      SkipStatus::NotSkippableRsync { mut op_iter } => {
        // Open the new file
        let mut new_file = staging_pool.get_writer(file_index)?;

        // Write all the new data into the file
        apply::patch_rsync(
          &mut op_iter,
          &mut new_file,
          new_file_size,
          src_pool,
          patch_op_buffer,
          checkpoint.current_file,
          |file_c| {
            // If a sync op was successfully applied,
            // save a checkpoint with the new data
            checkpoint.update_current_file_checkpoint(file_c);
            staging_pool.save_checkpoint(&checkpoint, false)
          },
          &mut progress_callback,
        )?
      }
      SkipStatus::NotSkippableBsdiff {
        target_index,
        mut op_iter,
      } => {
        // Open the new file
        let mut new_file = staging_pool.get_writer(file_index)?;

        // Write all the new data into the file
        apply::patch_bsdiff(
          &mut op_iter,
          target_index,
          &mut new_file,
          new_file_size,
          src_pool,
          patch_op_buffer,
          checkpoint.current_file,
          |file_c| {
            // If a sync op was successfully applied,
            // save a checkpoint with the new data
            checkpoint.update_current_file_checkpoint(file_c);
            staging_pool.save_checkpoint(&checkpoint, false)
          },
          &mut progress_callback,
        )?
      }
    };

    // Verify the patched file
    if let Some(hasher) = hasher {
      let mut reader = staging_pool.get_reader(file_index)?;
      let hash_status = hasher.hash_next_file(&mut reader)?;

      if let BlockHasherStatus::HashMismatch { block_index: _ } = hash_status {
        status = PatchFileStatus::Broken;
      }
    }

    // Update the checkpoint and save it
    checkpoint.push_status(status);
    staging_pool.save_checkpoint(&checkpoint, false)?;
  }

  Ok(ReconstructedFilesStatus {
    patched_files: checkpoint.patched_files,
  })
}
