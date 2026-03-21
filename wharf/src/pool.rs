mod errors;
mod null;
mod zip;

pub use errors::PoolError;
#[expect(unused_imports)]
pub use null::NullPool;
#[expect(unused_imports)]
pub use zip::ZipPool;

use std::io::Seek;
use std::io::{BufReader, Read};
use std::io::{BufWriter, Write};

/// Provides indexed read access to an ordered list of entries
///
/// Each entry is identified by its index in the pool's entry list.
#[expect(dead_code)]
pub trait Pool {
  /// The reader type returned by [`Pool::get_reader`]
  type Reader<'a>: Read
  where
    Self: 'a;

  /// Return the number of entries in this pool
  ///
  /// # Returns
  ///
  /// The number of entries in the pool, or [`usize::MAX`] if the pool is unbounded.
  /// Calling any other pool method with an index greater than or equal to this value
  /// will return a [`PoolError::InvalidEntryIndex`] error.
  fn entry_count(&self) -> usize;

  /// Return the size of the entry in the underlying storage
  ///
  /// # Returns
  ///
  /// - `Ok(None)` if the entry does not exist in the underlying storage.
  /// - `Ok(Some(size))` if the entry exists.
  ///
  /// # Errors
  ///
  /// If there is an I/O failure while querying the entry.
  fn get_size(&self, entry_index: usize) -> Result<Option<u64>, PoolError>;

  /// Return a reader for the entry at the given index
  ///
  /// # Returns
  ///
  /// A [`Self::Reader`] for the entry's contents.
  ///
  /// # Errors
  ///
  /// If the entry does not exist or there is an I/O failure while opening it.
  fn get_reader(&mut self, entry_index: usize) -> Result<Self::Reader<'_>, PoolError>;

  /// Return a buffered reader for the entry at the given index
  ///
  /// This is a convenience wrapper around [`Pool::get_reader`]
  fn get_bufreader(
    &mut self,
    entry_index: usize,
  ) -> Result<BufReader<Self::Reader<'_>>, PoolError> {
    self.get_reader(entry_index).map(BufReader::new)
  }
}

/// Extends [`Pool`] with seek access to the underlying storage.
///
/// This trait is automatically implemented for any [`Pool`] whose
/// [`Pool::Reader`] implements [`Seek`].
#[expect(dead_code)]
pub trait SeekablePool: Pool {
  /// The seekable reader type returned by [`SeekablePool::get_seek_reader`].
  type SeekableReader<'a>: Read + Seek
  where
    Self: 'a;

  /// Return a seekable reader for the entry at the given index
  ///
  /// # Returns
  ///
  /// A [`Self::SeekableReader`] that reads and seeks over the entry's contents.
  ///
  /// # Errors
  ///
  /// If the entry does not exist or there is an I/O failure while opening it.
  fn get_seek_reader(&mut self, entry_index: usize) -> Result<Self::SeekableReader<'_>, PoolError>;
}

/// Blanket implementation of [`SeekablePool`] for any [`Pool`] whose
/// [`Pool::Reader`] implements [`Seek`]
impl<P: Pool> SeekablePool for P
where
  for<'a> P::Reader<'a>: Seek,
{
  type SeekableReader<'a>
    = P::Reader<'a>
  where
    Self: 'a;

  fn get_seek_reader(&mut self, entry_index: usize) -> Result<Self::SeekableReader<'_>, PoolError> {
    self.get_reader(entry_index)
  }
}

/// Extends [`Pool`] with write access to the underlying storage
#[expect(dead_code)]
pub trait WritablePool: Pool {
  /// The writer type returned by [`WritablePool::get_writer`]
  type Writer<'a>: Write
  where
    Self: 'a;

  /// Truncate the entry at the given index to the specified size
  ///
  /// # Errors
  ///
  /// If the entry does not exist, there is an I/O failure while truncating it,
  /// or `size` is larger than the entry's current size in the underlying storage.
  fn truncate(&mut self, entry_index: usize, size: u64) -> Result<(), PoolError>;

  /// Return a writer for the entry at the given index
  ///
  /// If the entry does not exist, it will be created.
  /// The writer appends to the existing entry contents without truncating.
  /// To truncate first, call [`WritablePool::truncate`] before writing.
  ///
  /// # Errors
  ///
  /// If there is an I/O failure while opening the entry.
  fn get_writer(&mut self, entry_index: usize) -> Result<Self::Writer<'_>, PoolError>;

  /// Return a buffered writer for the entry at the given index.
  ///
  /// This is a convenience wrapper around [`WritablePool::get_writer`].
  fn get_bufwriter(
    &mut self,
    entry_index: usize,
  ) -> Result<BufWriter<Self::Writer<'_>>, PoolError> {
    self.get_writer(entry_index).map(BufWriter::new)
  }

  /// Copy an entry from the source pool into this pool
  ///
  /// This is a convenience wrapper around [`Pool::get_reader`] and
  /// [`WritablePool::get_writer`].
  ///
  /// # Returns
  ///
  /// The number of bytes copied.
  ///
  /// # Errors
  ///
  /// If there is an I/O failure while reading from the source pool,
  /// writing to this pool, or opening either entry.
  fn copy_from(&mut self, entry_index: usize, src: &mut impl Pool) -> Result<u64, PoolError> {
    let mut reader = src.get_reader(entry_index)?;
    let mut writer = self.get_writer(entry_index)?;
    Ok(std::io::copy(&mut reader, &mut writer)?)
  }
}
