use crate::hasher::BlockHasher;
use crate::patch::Patch;
use crate::patch::operations::FilesCache;
use crate::patch::operations::apply::{FileCheckpoint, PatchFileStatus};

use std::fs;
use std::io::Read;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
  pub fn push_status(&mut self, status: PatchFileStatus) {
    // Add the status to the vector of finished file patches
    self.patched_files.push(status);

    // Clear the current file checkpoint
    self.current_file = None;
  }
}

// Contains all the individual file patch status
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use]
pub struct ReconstructedFilesStatus {
  pub patched_files: Vec<PatchFileStatus>,
}

fn open_staging_writer(
  file_name: impl AsRef<Path>,
  staging_folder: &Path,
) -> Result<fs::File, String> {
  let file_path = staging_folder.join(file_name);

  fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(&file_path)
    .map_err(|e| {
      format!(
        "Couldn't open staging file in: \"{}\"\n{e}",
        file_path.to_string_lossy()
      )
    })
}

impl Patch<'_> {
  #[allow(clippy::too_many_arguments)]
  pub fn reconstruct_modified_files(
    &mut self,
    staging_folder: &Path,
    old_files_cache: &mut FilesCache,
    hasher: &mut Option<BlockHasher<impl Read>>,
    patch_op_buffer: &mut Vec<u8>,
    checkpoint: Option<StagingCheckpoint>,
    mut save_checkpoint: impl FnMut(&StagingCheckpoint),
    mut progress_callback: impl FnMut(u64),
  ) -> Result<ReconstructedFilesStatus, String> {
    // Get the default checkpoint (empty) if it doesn't exist
    // Because it is expensive to clone the checkpoint every time, it
    // is created here and reused for the whole function
    let mut checkpoint = checkpoint.unwrap_or_default();

    // Skip to the correct sync header
    self
      .sync_op_iter
      .skip_entries(checkpoint.patched_files.len() as u64)?;

    // Important!
    // Send save checkpoint calls every time:
    //
    // 1. A new sync op operation is successfully applied
    // 2. Any file is successfully fully patched (or skipped, etc.)
    //
    // The caller should decide whether to actually store those checkpoints

    // Patch all files in the iterator one by one
    while let Some(header) = self.sync_op_iter.next_header() {
      let mut header =
        header.map_err(|e| format!("Couldn't get next patch sync operation!\n{e}"))?;

      // Get the new file index
      let file_index = header.file_index as usize;

      // Open the new file
      let new_container_file = self.container_new.get_file(file_index)?;
      let new_file = || open_staging_writer(file_index.to_string(), staging_folder);

      // Create a hasher for the current file
      let mut file_hasher = match hasher.as_mut() {
        Some(h) => Some(h.new_file_hasher(new_container_file.block_count())?),
        None => None,
      };

      // Write all the new data into the file
      let status = header.patch_file(
        new_file,
        &mut file_hasher,
        new_container_file.size as u64,
        old_files_cache,
        &self.container_old,
        patch_op_buffer,
        checkpoint.current_file,
        |file_c| {
          // If a sync op was successfully applied,
          // save a checkpoint with the new data
          checkpoint.current_file = Some(file_c);
          save_checkpoint(&checkpoint);
        },
        &mut progress_callback,
      )?;

      // Update the checkpoint and save it
      checkpoint.push_status(status);
      save_checkpoint(&checkpoint);
    }

    Ok(ReconstructedFilesStatus {
      patched_files: checkpoint.patched_files,
    })
  }
}
