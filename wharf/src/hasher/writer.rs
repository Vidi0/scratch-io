use super::BlockHasher;
use crate::signature::read::BlockHashIter;

use std::io::{self, Read, Write};

pub struct HashWriter<'hr, 'w, HR, W> {
  writer: &'w mut W,
  hasher: BlockHasher<'hr, HR>,
}

impl<'hr, 'w, HR, W> HashWriter<'hr, 'w, HR, W> {
  pub fn new(writer: &'w mut W, hash_iter: &'hr mut BlockHashIter<HR>) -> Self {
    Self {
      writer,
      hasher: BlockHasher::new(hash_iter),
    }
  }
}

impl<'hr, 'w, HR: Read, W> HashWriter<'hr, 'w, HR, W> {
  pub fn finalize_block(&mut self) -> Result<(), String> {
    self.hasher.finalize_block()
  }
}

impl<HR: Read, W: Write> Write for HashWriter<'_, '_, HR, W> {
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
}
