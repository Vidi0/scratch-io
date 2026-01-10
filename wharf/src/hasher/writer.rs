use super::{BlockHasher, BlockHasherError};

use std::io::{self, Read, Write};

pub struct HashWriter<'h, 'h_iter, 'w, H, W> {
  writer: &'w mut W,
  hasher: &'h mut BlockHasher<'h_iter, H>,
}

impl<'h, 'h_iter, 'w, H, W> HashWriter<'h, 'h_iter, 'w, H, W> {
  pub fn new(writer: &'w mut W, hasher: &'h mut BlockHasher<'h_iter, H>) -> Self {
    Self { writer, hasher }
  }
}

impl<'h_iter, H> BlockHasher<'h_iter, H> {
  pub fn wrap_writer<'h, 'w, W>(
    &'h mut self,
    writer: &'w mut W,
  ) -> HashWriter<'h, 'h_iter, 'w, H, W> {
    HashWriter::new(writer, self)
  }
}

impl<'h, 'h_iter, 'w, H: Read, W> HashWriter<'h, 'h_iter, 'w, H, W> {
  pub fn finalize_block_and_reset(&mut self) -> Result<(), BlockHasherError> {
    self.hasher.finalize_block_and_reset()
  }
}

impl<HR: Read, W: Write> Write for HashWriter<'_, '_, '_, HR, W> {
  fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
    self.hasher.update(buf).map_err(io::Error::other)?;
    self.writer.write(buf)
  }

  fn flush(&mut self) -> std::io::Result<()> {
    self.writer.flush()
  }

  fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
    self.hasher.update(buf).map_err(io::Error::other)?;
    self.writer.write_all(buf)
  }

  fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
    for buf in bufs {
      self.hasher.update(buf).map_err(io::Error::other)?;
    }
    self.writer.write_vectored(bufs)
  }
}
