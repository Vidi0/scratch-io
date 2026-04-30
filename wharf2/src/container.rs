use crate::protos::{CompressionAlgorithm, CompressionSettings, Container, File};

use std::fmt::Display;

pub const BLOCK_SIZE: usize = 64 * 1024;

impl File {
  /// Get the number of blocks this [`File`] occupies
  pub fn blocks(&self) -> u64 {
    self.size.div_ceil(BLOCK_SIZE as u64).max(1)
  }
}

impl Display for CompressionAlgorithm {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      CompressionAlgorithm::None => write!(f, "uncompressed"),
      _ => write!(f, "{self:?}"),
    }
  }
}

impl Display for CompressionSettings {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self.algorithm {
      // If it is uncompressed, it doesn't make sense to add the quality
      CompressionAlgorithm::None => write!(f, "{}", self.algorithm),
      _ => write!(f, "{}-q{}", self.algorithm, self.quality),
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
