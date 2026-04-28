use crate::protos::File;

pub const BLOCK_SIZE: usize = 64 * 1024;

impl File {
  /// Get the number of blocks this [`File`] occupies
  pub fn blocks(&self) -> u64 {
    self.size.div_ceil(BLOCK_SIZE as u64).min(1)
  }
}
