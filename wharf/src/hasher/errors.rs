use thiserror::Error;

#[derive(Error, Debug)]
pub enum BlockHasherError {
  #[error("The file block hash iterator could not be obtained!")]
  CouldNotObtainIter(String),

  #[error("Expected block hash from iterator, got EOF!")]
  MissingHashFromIter,

  #[error(
    "The iterator returned an error:
{0}"
  )]
  IterReturnedError(String),

  #[error(
    "Could not get data from reader when hashing a file:
{0}"
  )]
  ReaderFailed(std::io::Error),

  #[error(
    "Could not get the expected file size from the container because it has run \
out of files at the index: {file_index}"
  )]
  RunOutOfFiles { file_index: usize },
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
  HashMismatch { block_index: usize },
}
