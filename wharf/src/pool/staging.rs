//! Staging pool implementation

use super::{Pool, PoolError, SeekablePool, WritablePool};

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

/// An unbounded writable pool backed by a folder on disk
///
/// Each entry is stored as a separate file in the base folder, named by its
/// index (e.g. `0`, `1`, `2`, ...). The pool is unbounded ([`Pool::entry_count`]
/// returns [`usize::MAX`]) so any index is valid for writing.
pub struct StagingPool<'path> {
  base_path: &'path Path,
}

impl<'path> StagingPool<'path> {
  pub fn new(base_path: &'path Path) -> Self {
    Self { base_path }
  }

  fn get_path(&self, entry_index: usize) -> PathBuf {
    self.base_path.join(entry_index.to_string())
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
