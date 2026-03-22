pub mod apply;
mod bsdiff;
mod rsync;

use crate::hasher::{BlockHasherError, BlockHasherStatus, FileBlockHasher};

use std::io::Read;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum OpStatus {
  Ok { written_bytes: u64 },
  Broken,
}

fn verify_data(
  hasher: &mut Option<FileBlockHasher<impl Read>>,
  data: &[u8],
) -> Result<OpStatus, BlockHasherError> {
  if let Some(hasher) = hasher
    && let BlockHasherStatus::HashMismatch { .. } = hasher.update(data)?
  {
    return Ok(OpStatus::Broken);
  }

  Ok(OpStatus::Ok {
    written_bytes: data.len() as u64,
  })
}
