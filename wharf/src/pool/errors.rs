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

impl From<PoolError> for String {
  fn from(value: PoolError) -> Self {
    value.to_string()
  }
}
