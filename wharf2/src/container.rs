use crate::protos::{CompressionSettings, Container, File};

use std::fmt::Display;

pub const BLOCK_SIZE: usize = 64 * 1024;

impl File {
  /// Get the number of blocks this [`File`] occupies
  pub fn blocks(&self) -> u64 {
    self.size.div_ceil(BLOCK_SIZE as u64).max(1)
  }
}

impl Display for CompressionSettings {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::None => write!(f, "uncompressed"),
      Self::Brotli { quality } => write!(f, "Brotli-q{quality}"),
      Self::Gzip { quality } => write!(f, "gzip-q{quality}"),
      Self::Zstd { quality } => write!(f, "Zstandard-q{quality}"),
    }
  }
}

impl Display for Container {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "{} files, {} dirs, {} symlinks, total size: {} bytes",
      self.files.len(),
      self.dirs.len(),
      self.symlinks.len(),
      self.size
    )
  }
}
