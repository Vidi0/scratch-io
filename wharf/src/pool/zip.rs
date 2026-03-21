use super::{Pool, PoolError};
use crate::protos::tlc;

use rc_zip_sync::{ArchiveHandle, EntryReader, HasCursor};
use std::io;

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
}

impl<'ar_reader, C: HasCursor> Pool for ZipPool<'_, '_, 'ar_reader, C> {
  type Reader<'a>
    = EntryReader<<C as HasCursor>::Cursor<'ar_reader>>
  where
    Self: 'a;

  fn entry_count(&self) -> usize {
    self.container.files.len()
  }

  fn get_size(&self, file_index: usize) -> Result<Option<u64>, PoolError> {
    let Some(container_file) = self.container.files.get(file_index) else {
      return Err(PoolError::InvalidEntryIndex(file_index));
    };

    Ok(
      self
        .archive
        .by_name(&container_file.path)
        .map(|entry| entry.uncompressed_size),
    )
  }

  fn get_reader(&mut self, file_index: usize) -> Result<Self::Reader<'_>, PoolError> {
    let Some(container_file) = self.container.files.get(file_index) else {
      return Err(PoolError::InvalidEntryIndex(file_index));
    };

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
