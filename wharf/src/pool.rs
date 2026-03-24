//! Pool abstractions for indexed file access
//!
//! This module provides traits for reading and writing files by index,
//! regardless of the underlying storage backend. Pools are used throughout
//! the patching and verification pipeline to abstract over different storage
//! types such as directories on disk, ZIP archives, and staging folders.
//!
//! # Traits
//!
//! - [`Pool`]: indexed read access
//! - [`ContainerBackedPool`]: extends [`Pool`] with access to container metadata
//! - [`SeekablePool`]: extends [`Pool`] with seek access
//! - [`WritablePool`]: extends [`Pool`] with write access
//!
//! # Implementations
//!
//! - [`ContainerPool`]: backed by a folder on disk, mirroring the structure
//!   of a wharf container
//! - [`NullPool`]: discards all writes and returns empty reads, useful for
//!   testing and benchmarking
//! - [`StagingPool`]: unbounded writable pool backed by a folder on disk
//! - [`ZipPool`]: backed by a ZIP archive

mod container;
mod errors;
mod null;
mod staging;
mod zip;

pub use container::ContainerPool;
pub use errors::PoolError;
pub use null::NullPool;
pub use staging::StagingPool;
pub use zip::ZipPool;

use std::io::Seek;
use std::io::{BufReader, Read};
use std::io::{BufWriter, Write};

/// Provides indexed read access to an ordered list of entries
///
/// Each entry is identified by its index in the pool's entry list.
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

/// Extends [`Pool`] with access to the container metadata
///
/// Implemented by pools that are backed by a wharf container,
/// allowing callers to query the declared size of entries as described
/// in the container metadata.
pub trait ContainerBackedPool: Pool {
  /// Return the size of the entry as declared in the container metadata
  ///
  /// # Returns
  ///
  /// The declared size of the entry in bytes.
  ///
  /// # Errors
  ///
  /// If `entry_index` is out of bounds for the pool, returns [`PoolError::InvalidEntryIndex`].
  fn get_container_size(&self, entry_index: usize) -> Result<u64, PoolError>;
}

/// Extends [`Pool`] with seek access to the underlying storage
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

/// Extends [`Pool`] with write access to the underlying storage
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
  /// The destination entry is truncated to zero before copying,
  /// so the final size equals the number of bytes copied.
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
    // Get the reader
    let mut reader = src.get_reader(entry_index)?;

    // Truncate the writer to 0 before getting the writer
    self.truncate(entry_index, 0)?;
    let mut writer = self.get_writer(entry_index)?;

    // Copy the data
    Ok(std::io::copy(&mut reader, &mut writer)?)
  }
}
