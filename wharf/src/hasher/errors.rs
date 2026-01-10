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

  #[error(
    "The hashes are not equal! The game files are going to be corrupted!
  Expected: {:X?}
  Got: {:X?}",
    expected,
    found
  )]
  HashMismatch {
    expected: Vec<u8>,
    found: [u8; MD5_HASH_LENGTH],
  },
}

impl From<BlockHasherError> for String {
  fn from(value: BlockHasherError) -> Self {
    value.to_string()
  }
}
