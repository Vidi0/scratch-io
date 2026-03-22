//! ZIP archive pool implementation

use super::{Pool, PoolError};
use crate::protos::tlc;

use rc_zip_sync::{ArchiveHandle, EntryReader, HasCursor};
use std::io;

/// A read-only pool backed by a ZIP archive
///
/// Each entry is looked up in the archive by its path as described in the
/// container. The number of entries is determined by the container's file list.
pub struct ZipPool<'container, 'ar, 'ar_reader, C: HasCursor>
where
  'ar: 'ar_reader,
{
  container: &'container tlc::Container,
  archive: &'ar ArchiveHandle<'ar_reader, C>,
}

impl<'container, 'ar, 'ar_reader, C: HasCursor> ZipPool<'container, 'ar, 'ar_reader, C> {
  pub fn new(
    container: &'container tlc::Container,
    archive: &'ar ArchiveHandle<'ar_reader, C>,
  ) -> Self {
    Self { container, archive }
  }

  fn get_file(&self, entry_index: usize) -> Result<&tlc::File, PoolError> {
    let Some(container_file) = self.container.files.get(entry_index) else {
      return Err(PoolError::InvalidEntryIndex(entry_index));
    };

    Ok(container_file)
  }
}

impl<'ar_reader, C: HasCursor> Pool for ZipPool<'_, '_, 'ar_reader, C> {
  type Reader<'a>
    = EntryReader<<C as HasCursor>::Cursor<'ar_reader>>
  where
    Self: 'a;

  fn entry_count(&self) -> usize {
    self.container.files.len()
  }

  fn get_size(&self, entry_index: usize) -> Result<Option<u64>, PoolError> {
    let container_file = self.get_file(entry_index)?;
    Ok(
      self
        .archive
        .by_name(&container_file.path)
        .map(|entry| entry.uncompressed_size),
    )
  }

  fn get_reader(&mut self, entry_index: usize) -> Result<Self::Reader<'_>, PoolError> {
    let container_file = self.get_file(entry_index)?;
    let filename = &*container_file.path;
    let entry = self.archive.by_name(filename).ok_or_else(|| {
      PoolError::Io(io::Error::new(
        io::ErrorKind::NotFound,
        format!("Expected to find the file by name in the ZIP build archive: \"{filename}\""),
      ))
    })?;

    Ok(entry.reader())
  }
}
