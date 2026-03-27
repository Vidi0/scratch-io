//! Staging pool implementation

use super::{Pool, PoolError, SeekablePool, WritablePool};

use serde::{Serialize, de::DeserializeOwned};
use std::fmt::Debug;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// An unbounded writable pool backed by a folder on disk
///
/// Each entry is stored as a separate file in the base folder, named by its
/// index (e.g. `0`, `1`, `2`, ...). The pool is unbounded ([`Pool::entry_count`]
/// returns [`usize::MAX`]) so any index is valid for writing.
pub struct StagingPool<'path> {
  base_path: &'path Path,

  // Store the instant the last checkpoint was saved to be able
  // to determine which checkpoints to skip and which ones to save
  last_checkpoint: Instant,
}

impl<'path> StagingPool<'path> {
  pub fn open(base_path: &'path Path) -> Self {
    Self {
      base_path,
      last_checkpoint: Instant::now(),
    }
  }

  pub fn create(base_path: &'path Path) -> Result<Self, PoolError> {
    fs::create_dir_all(base_path)?;
    Ok(Self {
      base_path,
      last_checkpoint: Instant::now(),
    })
  }

  fn get_path(&self, entry_index: usize) -> PathBuf {
    self.base_path.join(entry_index.to_string())
  }
}

impl StagingPool<'_> {
  // Store a checkpoint each second
  // Maybe it is too short?
  const CHECKPOINT_SAVE_INTERVAL: Duration = Duration::from_millis(1000);

  // These filenames won't collide with the files
  // because the files are stored by their entry index
  const CHECKPOINT_FILENAME: &'static str = "checkpoint";
  const CHECKPOINT_TEMP_FILENAME: &'static str = "checkpoint.tmp";

  fn get_checkpoint_path(&self) -> PathBuf {
    self.base_path.join(Self::CHECKPOINT_FILENAME)
  }

  fn get_checkpoint_temp_path(&self) -> PathBuf {
    self.base_path.join(Self::CHECKPOINT_TEMP_FILENAME)
  }

  pub fn save_checkpoint<T: Serialize + Debug>(
    &mut self,
    checkpoint: &T,
    force_save: bool,
  ) -> Result<(), String> {
    // Save the checkpoint only if the save interval time has passed
    if !force_save && self.last_checkpoint.elapsed() < Self::CHECKPOINT_SAVE_INTERVAL {
      return Ok(());
    }

    let str = serde_json::to_string(checkpoint)
      .map_err(|e| format!("Couldn't serialize checkpoint into JSON!\n{e}\n\n{checkpoint:?}"))?;

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

    self.last_checkpoint = Instant::now();

    Ok(())
  }

  pub fn load_checkpoint<T: DeserializeOwned>(&self) -> Result<Option<T>, String> {
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

    serde_json::from_str::<T>(&str)
      .map_err(|e| {
        format!(
          "Couldn't decode JSON checkpoint from: \"{}\"\n{e}\n\n{str}",
          path.display()
        )
      })
      .map(Some)
  }
}

impl Pool for StagingPool<'_> {
  type Reader<'a>
    = File
  where
    Self: 'a;

  fn entry_count(&self) -> usize {
    usize::MAX
  }

  fn get_size(&self, entry_index: usize) -> Result<Option<u64>, PoolError> {
    match fs::metadata(self.get_path(entry_index)) {
      Ok(m) => Ok(Some(m.len())),
      Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
      Err(e) => Err(e.into()),
    }
  }

  fn get_reader(&mut self, entry_index: usize) -> Result<Self::Reader<'_>, PoolError> {
    let path = self.get_path(entry_index);
    Ok(File::open(&path)?)
  }
}

impl SeekablePool for StagingPool<'_> {
  type SeekableReader<'a>
    = Self::Reader<'a>
  where
    Self: 'a;

  fn get_seek_reader(&mut self, entry_index: usize) -> Result<Self::SeekableReader<'_>, PoolError> {
    self.get_reader(entry_index)
  }
}

impl WritablePool for StagingPool<'_> {
  type Writer<'a>
    = File
  where
    Self: 'a;

  fn truncate(&mut self, entry_index: usize, size: u64) -> Result<(), PoolError> {
    let Some(current_size) = self.get_size(entry_index)? else {
      return Err(PoolError::Io(io::Error::new(
        io::ErrorKind::NotFound,
        "Couldn't truncate file in StagingPool if the file is missing!",
      )));
    };

    if current_size < size {
      return Err(PoolError::Io(io::Error::new(
        io::ErrorKind::InvalidInput,
        "Can't truncate file to a size greater than the current one!",
      )));
    }

    Ok(self.get_writer(entry_index)?.set_len(size)?)
  }

  fn get_writer(&mut self, entry_index: usize) -> Result<Self::Writer<'_>, PoolError> {
    let path = self.get_path(entry_index);
    Ok(OpenOptions::new().create(true).append(true).open(&path)?)
  }
}
