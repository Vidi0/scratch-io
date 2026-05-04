use std::convert::Infallible;
use std::io;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
  #[error(
    "invalid wharf binary:
{0}"
  )]
  InvalidBinary(#[from] InvalidBinary),

  #[error(
    "an IO error occured:
{0}"
  )]
  Io(#[from] IoError),
}

#[derive(Debug, Error)]
pub enum InvalidBinary {
  #[error("expected more bniary data, got EOF")]
  UnexpectedEOF,

  #[error("magic bytes mismatch: expected {expected}, found {found}")]
  MagicMismatch { expected: u32, found: u32 },

  #[error("magic bytes mismatch: does not match any known wharf binary format: {found}")]
  MagicNotFound { found: u32 },

  #[error("invalid protobuf length delimiter: {length_delimiter:?}")]
  InvalidLengthDelimiter { length_delimiter: Box<[u8]> },

  #[error(
    "could not decode protobuf message of type \"{message_type}\"
invalid protobuf message: {decode_error}
{bytes:?}"
  )]
  InvalidProto {
    message_type: &'static str,
    decode_error: String,
    bytes: Box<[u8]>,
  },

  #[error(
    "could not parse protobuf message of type \"{message_type}\"
{source}"
  )]
  InvalidField {
    message_type: &'static str,
    source: InvalidField,
  },
}

#[derive(Debug, Error)]
pub enum InvalidField {
  #[error("missing field: {field_name}")]
  MissingField { field_name: &'static str },

  #[error("expected valid usize, found: {int}")]
  ExpectedUsize { int: i64 },

  #[error("expected valid u64, found: {int}")]
  ExpectedU64 { int: i64 },

  #[error("expected vector length of {expected}, found length {found}")]
  ExpectedVecLength { expected: usize, found: usize },
}

impl InvalidField {
  /// Convert this [`InvalidField`] error into a generic [`enum@Error`].
  /// A `MessageType` type must be provided in order to add context to the error.
  pub fn into_error<MessageType>(self) -> Error {
    InvalidBinary::InvalidField {
      message_type: std::any::type_name::<MessageType>(),
      source: self,
    }
    .into()
  }
}

#[derive(Debug, Error)]
pub enum IoError {
  #[error("failed to read the wharf binary data: {0}")]
  WharfBinaryReadFailed(#[source] io::Error),

  #[error("failed to create a new Zstandard decoder: {0}")]
  CreateZstdDecoderFailed(#[source] io::Error),

  #[error("failed to write into dump debug message: {0}")]
  WriteDumpFailed(#[source] io::Error),
}

// This will never be called. It is added in order to satisfy the compiler
// until the never type is stabilized
impl From<Infallible> for Error {
  fn from(value: Infallible) -> Self {
    match value {}
  }
}
