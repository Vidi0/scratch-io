use thiserror::Error;

#[derive(Debug, Error)]
pub enum PoolError {
  #[error("The entry index is out of bounds for the pool: {0}")]
  InvalidEntryIndex(usize),

  #[error(
    "An I/O error occurred!
{0}"
  )]
  Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum PoolReadError {
  #[error("The entry index is out of bounds for the pool: {0}")]
  InvalidEntryIndex(usize),

  #[error(
    "An I/O error occurred!
{0}"
  )]
  Io(#[from] std::io::Error),

  #[error(
    "Entry not found in the underlying storage at index: {index}
{message}"
  )]
  NotFound { index: usize, message: String },
}

impl From<PoolError> for PoolReadError {
  fn from(value: PoolError) -> Self {
    match value {
      PoolError::InvalidEntryIndex(v) => Self::InvalidEntryIndex(v),
      PoolError::Io(v) => Self::Io(v),
    }
  }
}

impl From<PoolError> for String {
  fn from(value: PoolError) -> Self {
    value.to_string()
  }
}

impl From<PoolReadError> for String {
  fn from(value: PoolReadError) -> Self {
    value.to_string()
  }
}
