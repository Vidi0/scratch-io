use crate::hasher::{BlockHasher, BlockHasherStatus};
use crate::patch::operations::{
  apply::{self, FileCheckpoint, PatchFileStatus},
  skip::SkipStatus,
};
use crate::patch::{SyncEntryIter, SyncHeader};
use crate::pool::{ContainerBackedPool, Pool, SeekablePool, StagingPool, WritablePool};

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::sync::mpsc::{self, Receiver, SyncSender};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub struct StagingCheckpoint {
  /// A vector containing in order all the files that have been
  /// successfully patched
  patched_files: Vec<PatchFileStatus>,

  /// A checkpoint representing the file that is currently
  /// being patched
  current_file: Option<FileCheckpoint>,

  /// The number of files that have been verified (successfully or not).
  ///
  /// Only files that have acually been patched will be verified. For that reason,
  /// [`Self::verified_files`] is always lower or equal to the number of files in
  /// [`Self::patched_files`] with status [`PatchFileStatus::Patched`] or
  /// [`PatchFileStatus::VerificationFailed`].
  verified_files: usize,
}

impl StagingCheckpoint {
  /// Returns the index of the file that has to be patched
  /// or was being patched
  pub fn current_file_index(&self) -> u64 {
    self.patched_files.len() as u64
  }

  /// Return the number of files that have been patched (the status is
  /// [`PatchFileStatus::Patched`] or [`PatchFileStatus::VerificationFailed`])
  pub fn patched_files_count(&self) -> usize {
    self.patched_files.iter().fold(0usize, |acc, status| {
      // If the file has been patched, add 1 to the count
      if status.has_been_patched() {
        acc + 1
      } else {
        acc
      }
    })
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

  /// Must be called after [`StagingCheckpoint::push_status`]
  pub fn store_verification(&mut self, file_index: usize, is_broken: bool) {
    self.verified_files += 1;

    if is_broken {
      self.patched_files[file_index] = PatchFileStatus::VerificationFailed;
    }
  }

  /// Load the checkpoint
  pub fn load(&self, sync_op_iter: &mut SyncEntryIter) -> Result<(), String> {
    if self.current_file_index() == 0 {
      return Ok(());
    }

    // Skip to the correct sync header
    sync_op_iter.skip_entries(self.current_file_index())?;

    Ok(())
  }
}

// Contains all the individual file patch status
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use]
pub struct ReconstructedFilesStatus {
  pub patched_files: Vec<PatchFileStatus>,
}

/// Apply the patch entry and patch the file into `staging_pool` at the index
/// provided in the `header`.
///
/// The patch entry may not be applied if it is determined not to be necessary
/// (either because it is empty or a literal copy of one in the old container).
///
/// # Returns
///
/// Whether the file was actually written to `staging_pool`, or an error.
fn patch_file(
  header: SyncHeader,
  src_pool: &mut (impl SeekablePool + ContainerBackedPool),
  staging_pool: &mut StagingPool,
  patch_op_buffer: &mut Vec<u8>,
  new_file_size: u64,
  checkpoint: &mut StagingCheckpoint,
  progress_callback: impl FnMut(u64) + Send,
) -> Result<PatchFileStatus, String> {
  // Get the entry index from the header before calling check_skip
  let entry_index = header.file_index;

  // Before patching, check if the file really needs patching
  match header.check_skip(new_file_size, src_pool)? {
    SkipStatus::Empty => Ok(PatchFileStatus::Empty),
    SkipStatus::LiteralCopy { old_index } => Ok(PatchFileStatus::LiteralCopy { old_index }),
    SkipStatus::NotSkippableRsync { mut op_iter } => {
      // Open the new file
      let mut new_file = staging_pool.get_writer(entry_index)?;

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
        progress_callback,
      )
    }
    SkipStatus::NotSkippableBsdiff {
      target_index,
      mut op_iter,
    } => {
      // Open the new file
      let mut new_file = staging_pool.get_writer(entry_index)?;

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
        progress_callback,
      )
    }
  }
}

struct PatchedFileInfo<'checkpoint, 'pool, 'pool_path> {
  file_index: usize,
  status: PatchFileStatus,
  checkpoint: &'checkpoint mut StagingCheckpoint,
  staging_pool: &'pool mut StagingPool<'pool_path>,
}

/// Shared patching logic
///
/// Calls `on_file_patched` after each file is patched with its status.
/// The status with be returned in order. In order to allow the callback
/// to read the patched file and update the checkpoint, a mutable reference
/// to the [`StagingPool`] is provided. The callback decides what to do
/// with the status (verify it, store it, etc.)
///
/// The information about the patching result must be retrieved from the
/// `checkpoint` argument, which will be mutated.
#[expect(clippy::too_many_arguments)]
fn reconstruct_files_common<F>(
  checkpoint: &mut StagingCheckpoint,
  src_pool: &mut (impl SeekablePool + ContainerBackedPool),
  staging_pool: &mut StagingPool,
  dst_pool: &mut impl ContainerBackedPool,
  sync_op_iter: &mut SyncEntryIter,
  patch_op_buffer: &mut Vec<u8>,
  mut progress_callback: impl FnMut(u64) + Send,
  mut on_file_patched: F,
) -> Result<(), String>
where
  F: FnMut(PatchedFileInfo) -> Result<(), String>,
{
  // Load the checkpoint
  checkpoint.load(sync_op_iter)?;

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

    // Patch the file (or skip patching instead if it is not needed)
    let status = patch_file(
      header,
      src_pool,
      staging_pool,
      patch_op_buffer,
      new_file_size,
      checkpoint,
      &mut progress_callback,
    )?;

    // Return the status and let the caller decide what to do with it
    on_file_patched(PatchedFileInfo {
      file_index,
      status,
      checkpoint,
      staging_pool,
    })?;
  }

  Ok(())
}

fn reconstruct_without_verification(
  src_pool: &mut (impl SeekablePool + ContainerBackedPool),
  staging_pool: &mut StagingPool,
  dst_pool: &mut impl ContainerBackedPool,
  sync_op_iter: &mut SyncEntryIter,
  patch_op_buffer: &mut Vec<u8>,
  progress_callback: impl FnMut(u64) + Send,
) -> Result<ReconstructedFilesStatus, String> {
  // Deserialize the last checkpoint stored in the staging folder
  // Get the default (empty) checkpoint if it does not exist
  let mut checkpoint = staging_pool
    .load_checkpoint::<StagingCheckpoint>()?
    .unwrap_or_default();

  let on_file_patched = |info: PatchedFileInfo| {
    // Update the checkpoint and save it
    info.checkpoint.push_status(info.status);
    info.staging_pool.save_checkpoint(&info.checkpoint, false)
  };

  reconstruct_files_common(
    &mut checkpoint,
    src_pool,
    staging_pool,
    dst_pool,
    sync_op_iter,
    patch_op_buffer,
    progress_callback,
    on_file_patched,
  )?;

  Ok(ReconstructedFilesStatus {
    patched_files: checkpoint.patched_files,
  })
}

/// File ready for verification
#[derive(Debug)]
struct FileToVerify {
  file_index: usize,
  reader: File,
}

/// Verification result from the verification thread
#[derive(Debug)]
struct VerificationResult {
  file_index: usize,
  status: BlockHasherStatus,
}

fn verify_files_thread(
  hasher: &mut BlockHasher,
  file_receiver: Receiver<FileToVerify>,
  verification_status_sender: SyncSender<VerificationResult>,
) -> Result<(), String> {
  loop {
    // Receive the next file to verify from the channel, blocking if necessary
    let Ok(FileToVerify {
      file_index,
      mut reader,
    }) = file_receiver.recv()
    else {
      // If the sender has disconnected, the verification has finished.
      return Ok(());
    };

    // Verify the file
    let status = hasher.hash_next_file(&mut reader, file_index, |_| ())?;

    // Send the status through the channel back to the main thread
    verification_status_sender
      .send(VerificationResult { file_index, status })
      .expect("The main thread must NOT hung up until receiving all the hasher status!");
  }
}

fn send_file_to_hasher(
  file_index: usize,
  staging_pool: &mut StagingPool,
  file_sender: &SyncSender<FileToVerify>,
) -> Result<(), String> {
  // Get the reader
  let reader = staging_pool.get_reader(file_index)?;

  // Send the file to the hasher
  file_sender
    .send(FileToVerify { file_index, reader })
    .expect("The hasher thread must NOT hung up until we do it!");

  Ok(())
}

/// Send file that have not been sent yet to the hasher thread but
/// have been already patched
///
/// This is needed because, after loading a checkpoint, some files might have been patched
/// but their verification might not have been completed. This function will send all those
/// files into the hasher thread.
///
/// Because files are verified in order, the index of the files that haven't been verified yet
/// can be determined.
fn send_available_files_to_verify(
  checkpoint: &mut StagingCheckpoint,
  staging_pool: &mut StagingPool,
  file_sender: &SyncSender<FileToVerify>,
) -> Result<(), String> {
  // For each file in the checkpoint (and its index):
  //
  // 1. Remove the files that have not been patched from the iterator
  // 2. Skip the files that have already been verified
  // 3. Send the remaining files to the hasher
  checkpoint
    .patched_files
    .iter()
    .enumerate()
    .filter_map(|(index, status)| status.has_been_patched().then_some(index))
    .skip(checkpoint.verified_files)
    .try_for_each(|file_index| send_file_to_hasher(file_index, staging_pool, file_sender))
}

fn handle_verification_results(
  verification_status_receiver: &Receiver<VerificationResult>,
  checkpoint: &mut StagingCheckpoint,
  staging_pool: &mut StagingPool,
  blocking: bool,
) -> Result<(), String> {
  let mut was_checkpoint_modified = false;

  loop {
    let hash_status = if blocking {
      // In blocking mode, wait for all the verification status
      match verification_status_receiver.recv() {
        Ok(status) => status,
        // The channel has been disconnected, so no more messages
        // will be received
        Err(_) => break,
      }
    } else {
      // In non blocking mode, retrieve the verification status for
      // the files that have already been verified
      match verification_status_receiver.try_recv() {
        Ok(status) => status,
        // Don't block on empty channel, just return
        Err(mpsc::TryRecvError::Empty) => break,
        Err(mpsc::TryRecvError::Disconnected) => {
          panic!("The hasher thread must NOT hung up when handling results in non blocking mode!")
        }
      }
    };

    // Check the status and store it into the checkpoint
    let is_broken = hash_status.status.is_broken();

    checkpoint.store_verification(hash_status.file_index, is_broken);
    was_checkpoint_modified = true;
  }

  if was_checkpoint_modified {
    staging_pool.save_checkpoint(checkpoint, false)?;
  }

  Ok(())
}

pub fn reconstruct_with_verification(
  src_pool: &mut (impl SeekablePool + ContainerBackedPool),
  staging_pool: &mut StagingPool,
  dst_pool: &mut impl ContainerBackedPool,
  sync_op_iter: &mut SyncEntryIter,
  hasher: &mut BlockHasher,
  patch_op_buffer: &mut Vec<u8>,
  progress_callback: impl FnMut(u64) + Send,
) -> Result<ReconstructedFilesStatus, String> {
  // Deserialize the last checkpoint stored in the staging folder
  // Get the default (empty) checkpoint if it does not exist
  let mut checkpoint = staging_pool
    .load_checkpoint::<StagingCheckpoint>()?
    .unwrap_or_default();

  // The sync op iterator hasn't been advanced yet, so the number of remaining
  // entries is equal to the number of total files to patch
  let files_to_patch = sync_op_iter.remaining_entries as usize;

  let (file_sender, file_receiver) = mpsc::sync_channel::<FileToVerify>(files_to_patch);
  let (verification_status_sender, verification_status_receiver) =
    mpsc::sync_channel::<VerificationResult>(files_to_patch);

  // Store the receiver as a reference
  let verification_status_receiver = &verification_status_receiver;

  // Send the files that have been patched but not verifyed into the hasher thread
  send_available_files_to_verify(&mut checkpoint, staging_pool, &file_sender)?;

  // Move clousure to drop the file sender when it goes out of scope
  //
  // The verification status receiver is passed into the closure as a reference to allow using it
  // later to finish the verficiation after patching all the files.
  let on_file_patched = move |info: PatchedFileInfo| {
    // Update the checkpoint
    info.checkpoint.push_status(info.status);

    // If the file has been patched, send it though the channel in order for
    // the hasher thread to hash it
    if let PatchFileStatus::Patched { .. } = info.status {
      // Send the file to the hasher
      send_file_to_hasher(info.file_index, info.staging_pool, &file_sender)?;

      // Check if the hasher thread has finished verifying any new files
      handle_verification_results(
        verification_status_receiver,
        info.checkpoint,
        info.staging_pool,
        false,
      )?;
    }

    Ok(())
  };

  std::thread::scope(|scope| -> Result<ReconstructedFilesStatus, String> {
    // Spawn the hasher thread
    let hasher_handle =
      scope.spawn(|| verify_files_thread(hasher, file_receiver, verification_status_sender));

    // Patch the files, passing the verified ones to the hasher thread
    reconstruct_files_common(
      &mut checkpoint,
      src_pool,
      staging_pool,
      dst_pool,
      sync_op_iter,
      patch_op_buffer,
      progress_callback,
      on_file_patched,
    )?;

    // Check the verification status of the remaining files
    handle_verification_results(
      verification_status_receiver,
      &mut checkpoint,
      staging_pool,
      true,
    )?;

    // Wait for the hasher thread to stop
    hasher_handle.join().unwrap()?;

    // Assert the number of patched files and verified files is the same
    let patched_files_count = checkpoint.patched_files_count();
    assert_eq!(patched_files_count, checkpoint.verified_files);

    Ok(ReconstructedFilesStatus {
      patched_files: checkpoint.patched_files,
    })
  })
}

pub fn reconstruct_modified_files(
  src_pool: &mut (impl SeekablePool + ContainerBackedPool),
  staging_pool: &mut StagingPool,
  dst_pool: &mut impl ContainerBackedPool,
  sync_op_iter: &mut SyncEntryIter,
  hasher: &mut Option<BlockHasher>,
  patch_op_buffer: &mut Vec<u8>,
  progress_callback: impl FnMut(u64) + Send,
) -> Result<ReconstructedFilesStatus, String> {
  match hasher {
    None => reconstruct_without_verification(
      src_pool,
      staging_pool,
      dst_pool,
      sync_op_iter,
      patch_op_buffer,
      progress_callback,
    ),
    Some(hasher) => reconstruct_with_verification(
      src_pool,
      staging_pool,
      dst_pool,
      sync_op_iter,
      hasher,
      patch_op_buffer,
      progress_callback,
    ),
  }
}
