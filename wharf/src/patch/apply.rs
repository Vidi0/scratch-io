mod staging;

use super::Patch;
use super::operations::FilesCache;
use crate::hasher::BlockHasher;
use crate::signature::BlockHashIter;

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

// Store a checkpoint each second
// Maybe it is too short?
const CHECKPOINT_SAVE_INTERVAL: Duration = Duration::from_millis(1000);

pub struct StagingFiles<'a> {
  staging_folder: &'a Path,
}

impl<'a> StagingFiles<'a> {
  const CHECKPOINT_FILENAME: &'static str = "checkpoint";
  const CHECKPOINT_TEMP_FILENAME: &'static str = "checkpoint.tmp";

  fn get_checkpoint_path(&self) -> PathBuf {
    self.staging_folder.join(Self::CHECKPOINT_FILENAME)
  }

  fn get_checkpoint_temp_path(&self) -> PathBuf {
    self.staging_folder.join(Self::CHECKPOINT_TEMP_FILENAME)
  }

  pub fn save_checkpoint(
    &self,
    checkpoint: &staging::StagingCheckpoint,
    last_checkpoint_instant: &mut std::time::Instant,
  ) -> Result<(), String> {
    // Save the checkpoint only if the save interval time has passed
    if last_checkpoint_instant.elapsed() < CHECKPOINT_SAVE_INTERVAL {
      return Ok(());
    }

    let str = serde_json::to_string(checkpoint)
      .map_err(|e| format!("Couldn't serialize checkpoint into TOML!\n{e}\n\n{checkpoint:?}"))?;

    // Save the new checkpoint to a temp file, and then
    // do an atomic rename to replace the old checkpoint
    let temp_path = self.get_checkpoint_temp_path();
    let final_path = self.get_checkpoint_path();

    fs::write(&temp_path, str).map_err(|e| {
      format!(
        "Couldn't save data to checkpoint: \"{}\"\n{e}",
        temp_path.display()
      )
    })?;

    // Data has been writte, now do the atomic replace
    fs::rename(&temp_path, &final_path)
      .map_err(|e| format!("Couldn't move checkpoint from temp to final destination!\n{e}"))?;

    *last_checkpoint_instant = std::time::Instant::now();

    Ok(())
  }

  pub fn load_checkpoint(&self) -> Result<Option<staging::StagingCheckpoint>, String> {
    let path = self.get_checkpoint_path();

    // If the checkpoint doesn't exist, return None
    if !path.try_exists().map_err(|e| {
      format!(
        "Couldn't check if checkpoint exists: \"{}\"\n{e}",
        path.display()
      )
    })? {
      return Ok(None);
    }

    // Else, decode it
    let str = std::fs::read_to_string(&path)
      .map_err(|e| format!("Couldn't open checkpoint file: \"{}\"\n{e}", path.display()))?;

    serde_json::from_str(&str).map_err(|e| {
      format!(
        "Couldn't decode TOML checkpoint from: \"{}\"\n{e}\n\n{str}",
        path.display()
      )
    })
  }

  fn get_file_path(&self, file_index: usize) -> PathBuf {
    self.staging_folder.join(file_index.to_string())
  }

  pub fn open_write(&self, file_index: usize) -> Result<fs::File, String> {
    let file_path = self.get_file_path(file_index);

    // Don't set `create_new`!
    // If a file is half-patched, the patcher should be able
    // to load the previous file and truncate it!
    //
    // Set `read` to be able to hash the last block from a checkpoint
    fs::OpenOptions::new()
      .create(true)
      .append(true)
      .read(true)
      .open(&file_path)
      .map_err(|e| {
        format!(
          "Couldn't open staging file to write in: \"{}\"\n{e}",
          file_path.display()
        )
      })
  }

  pub fn open_read(&self, file_index: usize) -> Result<fs::File, String> {
    let file_path = self.get_file_path(file_index);
    fs::File::open(&file_path).map_err(|e| {
      format!(
        "Couldn't open staging file to read from: \"{}\"\n{e}",
        file_path.display()
      )
    })
  }
}

impl Patch<'_> {
  /// Apply the patch operations to produce the new build.
  ///
  /// This creates all files, directories, and symlinks in `new_build_folder`,
  /// then applies each sync operation (rsync or bsdiff) using data from
  /// `old_build_folder`. Written data is hashed on the fly and verified against
  /// `hash_iter` (if provided). `progress_callback` is invoked with the number
  /// of processed bytes as the patch is applied.
  ///
  /// # Arguments
  ///
  /// * `old_build_folder` - The path to the old build folder
  ///
  /// * `new_build_folder` - The path to the new build folder
  ///
  /// * `hash_iter` - Iterator over expected block hashes used to verify the
  ///   integrity of the written files (optional)
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
    staging_folder: &Path,
    new_build_folder: &Path,
    hash_iter: Option<&mut BlockHashIter<impl Read>>,
    mut progress_callback: impl FnMut(u64),
  ) -> Result<(), String> {
    // Create the new container folders, files and symlinks,
    // applying all the correct permissions
    self.container_new.create(new_build_folder)?;

    // Create the staging folder
    fs::create_dir_all(staging_folder).map_err(|e| {
      format!(
        "Couldn't create staging folder: \"{}\"\n{e}",
        staging_folder.display()
      )
    })?;

    // Create a cache of open file descriptors for the old files
    // The key is the file_index of the old file provided by the patch
    // The value is the open file descriptor
    let mut old_files_cache = FilesCache::new(old_build_folder);

    // This buffer is used when applying rsync block_range operations and
    // bsdiff add operations. It is created here to avoid allocating and
    // deallocating the buffer on each patch operation.
    //
    // If the buffer was used by the two operations at the same time, it
    // would be very expensive to resize the vector every time the kind of
    // operation changes: block_range operations use a fixed size set by us;
    // while the add operations' length is provided by the patch (every
    // operation should be the same size).
    //
    // However, the buffer can be shared between the two because the patch
    // is either a rsync patch, and only block_range operations are used;
    // or is a bsdiff patch, and then only add operations are used (the patch
    // may contain block_range operations, but for copying whole files
    // unchanged, so a buffer isn't needed anyways).
    let mut patch_op_buffer: Vec<u8> = Vec::new();

    // If a hash_iter was provided, create a reusable hasher
    // instance to verify that the new game files are intact
    let mut hasher = hash_iter.map(|iter| BlockHasher::new(iter));

    // Create a struct that allows the `reconstruct_modified_files` function
    // to store the patched files in the staging folder
    let staging = StagingFiles { staging_folder };

    // Deserialize the last checkpoint stored in the staging folder
    let checkpoint = staging.load_checkpoint()?;

    // Store the instant the last checkpoint was saved to be able
    // to determine which checkpoints to skip and which ones to save
    let mut last_checkpoint_instant = std::time::Instant::now();

    // Reconstruct all the modified files into the staging folder
    let status = self.reconstruct_modified_files(
      &staging,
      &mut old_files_cache,
      &mut hasher,
      &mut patch_op_buffer,
      checkpoint,
      |checkpoint| staging.save_checkpoint(checkpoint, &mut last_checkpoint_instant),
      &mut progress_callback,
    )?;

    ///////// TODO: do something with the status
    for (file_index, file_status) in status.patched_files.into_iter().enumerate() {
      println!("file {}: {:?}", file_index, file_status);
    }

    Ok(())
  }
}
