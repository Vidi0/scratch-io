use crate::common::block_count;
use crate::protos;

impl std::fmt::Display for protos::CompressionSettings {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{:?}-q{}", self.algorithm(), self.quality)
  }
}

impl protos::File {
  /// Get the number of blocks that the file occupies
  ///
  /// If the file is empty, still count one block for its empty hash
  #[inline]
  #[must_use]
  pub fn block_count(&self) -> u64 {
    block_count(self.size as u64)
  }
}

impl protos::Container {
  pub fn dump_stdout(&self) {
    // Print the container size
    println!("{}", self.size);

    // Print every file, directory and symlink
    for file in &self.files {
      println!("{file:?}");
    }
    for dir in &self.dirs {
      println!("{dir:?}");
    }
    for sym in &self.symlinks {
      println!("{sym:?}");
    }
  }

  pub fn print_summary(&self, label: &str) {
    println!(
      "{label}: {} files, {} dirs, {} symlinks, total size: {} bytes",
      self.files.len(),
      self.dirs.len(),
      self.symlinks.len(),
      self.size,
    );
  }

  /// Get the number of blocks every file in this container
  /// combined occupies
  ///
  /// If a file is empty, still count one block for its empty hash
  #[inline]
  #[must_use]
  pub fn file_blocks(&self) -> u64 {
    self.files.iter().fold(0, |acc, f| acc + f.block_count())
  }
}
