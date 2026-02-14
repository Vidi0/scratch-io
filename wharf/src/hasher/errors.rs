use super::MD5_HASH_LENGTH;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum BlockHasherError {
  #[error("Expected block hash from iterator, got EOF!")]
  MissingHashFromIter,

  #[error(
    "The iterator returned an error:
{0}"
  )]
  IterReturnedError(String),

  #[error("All the current file blocks have been hashed. No more data for this file is allowed!")]
  AllBlocksHashed,
}

impl From<BlockHasherError> for String {
  fn from(value: BlockHasherError) -> Self {
    value.to_string()
  }
}

#[must_use]
#[derive(Clone, Debug)]
pub enum BlockHasherStatus {
  Ok,
  HashMismatch {
    expected: Vec<u8>,
    found: [u8; MD5_HASH_LENGTH],
  },
}
