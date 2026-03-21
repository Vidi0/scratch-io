use super::{Pool, PoolError, WritablePool};

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

pub struct StagingPool<'path> {
  base_path: &'path Path,
}

impl<'path> StagingPool<'path> {
  pub fn new(base_path: &'path Path) -> Self {
    Self { base_path }
  }

  fn get_path(&self, file_index: usize) -> PathBuf {
    self.base_path.join(file_index.to_string())
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

  fn get_size(&self, file_index: usize) -> Result<Option<u64>, PoolError> {
    match self.get_path(file_index).metadata() {
      Ok(m) => Ok(Some(m.len())),
      Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
      Err(e) => Err(e.into()),
    }
  }

  fn get_reader(&mut self, file_index: usize) -> Result<Self::Reader<'_>, PoolError> {
    let path = self.get_path(file_index);

    Ok(File::open(&path)?)
  }
}

impl WritablePool for StagingPool<'_> {
  type Writer<'a>
    = File
  where
    Self: 'a;

  fn get_writer(&mut self, file_index: usize) -> Result<Self::Writer<'_>, PoolError> {
    let path = self.get_path(file_index);

    Ok(OpenOptions::new().create(true).append(true).open(&path)?)
  }

  fn truncate(&mut self, file_index: usize, size: u64) -> Result<(), PoolError> {
    let Some(current_size) = self.get_size(file_index)? else {
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

    Ok(self.get_writer(file_index)?.set_len(size)?)
  }
}
