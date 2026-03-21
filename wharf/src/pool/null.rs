//! Null pool implementation

use super::{Pool, PoolError, WritablePool};

use std::io;
use std::io::{BufReader, BufWriter};

/// A pool that discards all writes and returns empty reads
///
/// Represents an unbounded sequence of empty entries: every entry exists,
/// has size zero, and discards any data written to it.
///
/// Useful for testing and benchmarking when actual I/O is not needed.
pub struct NullPool;

impl Pool for NullPool {
  type Reader<'a>
    = io::Empty
  where
    Self: 'a;

  fn entry_count(&self) -> usize {
    usize::MAX
  }

  fn get_size(&self, _entry_index: usize) -> Result<Option<u64>, PoolError> {
    Ok(Some(0))
  }

  fn get_reader(&mut self, _entry_index: usize) -> Result<Self::Reader<'_>, PoolError> {
    Ok(io::empty())
  }

  fn get_bufreader(
    &mut self,
    _entry_index: usize,
  ) -> Result<io::BufReader<Self::Reader<'_>>, PoolError> {
    // Set the capacity to 0 to avoid wasting memory
    Ok(BufReader::with_capacity(0, io::empty()))
  }
}

impl WritablePool for NullPool {
  type Writer<'a>
    = io::Empty
  where
    Self: 'a;

  fn truncate(&mut self, _entry_index: usize, _size: u64) -> Result<(), PoolError> {
    Ok(())
  }

  fn get_writer(&mut self, _entry_index: usize) -> Result<Self::Writer<'_>, PoolError> {
    Ok(io::empty())
  }

  fn get_bufwriter(
    &mut self,
    _entry_index: usize,
  ) -> Result<io::BufWriter<Self::Writer<'_>>, PoolError> {
    // Set the capacity to 0 to avoid wasting memory
    Ok(BufWriter::with_capacity(0, io::empty()))
  }

  fn copy_from(&mut self, _entry_index: usize, _src: &mut impl Pool) -> Result<u64, PoolError> {
    Ok(0)
  }
}
