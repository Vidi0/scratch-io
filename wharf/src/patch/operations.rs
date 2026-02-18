mod bsdiff;
mod rsync;

use crate::container::OpenFileStatus;
use crate::hasher::{BlockHasherError, BlockHasherStatus, FileBlockHasher};
use crate::protos::tlc;

use std::fs;
use std::io::Read;
use std::path::Path;

const MAX_OPEN_FILES_PATCH: std::num::NonZeroUsize = std::num::NonZeroUsize::new(16).unwrap();

#[must_use]
pub enum FilesCacheStatus<'a> {
  Ok {
    file: &'a mut fs::File,
    container_size: u64,
    disk_size: u64,
  },
  NotFound,
}

pub struct FilesCache<'a> {
  cache: lru::LruCache<usize, (fs::File, u64, u64)>,
  build_folder: &'a Path,
}

impl<'a> FilesCache<'a> {
  pub fn new(build_folder: &'a Path) -> Self {
    FilesCache {
      cache: lru::LruCache::new(MAX_OPEN_FILES_PATCH),
      build_folder,
    }
  }

  pub fn get_file(
    &mut self,
    index: usize,
    container: &tlc::Container,
  ) -> Result<FilesCacheStatus<'_>, String> {
    enum CacheResult {
      Error(String),
      NotFound,
    }

    let result = self.cache.try_get_or_insert_mut(index, || {
      match container.open_file_read(index, self.build_folder.to_owned()) {
        Err(e) => Err(CacheResult::Error(e)),
        Ok(OpenFileStatus::NotFound) => Err(CacheResult::NotFound),
        Ok(OpenFileStatus::Ok {
          file,
          container_size,
          disk_size,
        }) => Ok((file, container_size, disk_size)),
      }
    });

    match result {
      Ok((file, container_size, disk_size)) => Ok(FilesCacheStatus::Ok {
        file,
        container_size: *container_size,
        disk_size: *disk_size,
      }),
      Err(CacheResult::NotFound) => Ok(FilesCacheStatus::NotFound),
      Err(CacheResult::Error(e)) => Err(e),
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum OpStatus {
  Ok { written_bytes: u64 },
  Broken,
}

fn verify_data(
  hasher: &mut Option<FileBlockHasher<impl Read>>,
  data: &[u8],
) -> Result<OpStatus, BlockHasherError> {
  if let Some(hasher) = hasher
    && let BlockHasherStatus::HashMismatch { .. } = hasher.update(data)?
  {
    return Ok(OpStatus::Broken);
  }

  Ok(OpStatus::Ok {
    written_bytes: data.len() as u64,
  })
}
