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
  InvalidWharfBinary(#[from] InvalidWharfBinary),

  #[error(
    "an IO error occured:
{0}"
  )]
  Io(#[from] IoError),
}

#[derive(Debug, Error)]
pub enum InvalidWharfBinary {
  #[error("expected more bniary data, got EOF")]
  UnexpectedEOF,

  #[error("magic bytes mismatch: expected {expected}, found {found}")]
  MagicMismatch { expected: u32, found: u32 },

  #[error("invalid protobuf length delimiter: {length_delimiter:?}")]
  InvalidLengthDelimiter { length_delimiter: Box<[u8]> },

  #[error(
    "could not parse protobuf message of type \"{message_type}\"
{source}"
  )]
  InvalidMessage {
    message_type: &'static str,
    source: InvalidWharfMessage,
  },
}

#[derive(Debug, Error)]
pub enum InvalidWharfMessage {
  #[error(
    "invalid protobuf message: {decode_error}
{bytes:?}"
  )]
  InvalidProtoMessage {
    decode_error: String,
    bytes: Box<[u8]>,
  },

  #[error("missing field: {field_name}")]
  MissingProtoField { field_name: &'static str },

  #[error("expected valid usize, found: {int}")]
  ExpectedUsize { int: i64 },

  #[error("expected valid u64, found: {int}")]
  ExpectedU64 { int: i64 },

  #[error("expected vector length of {expected}, found length {found}")]
  ExpectedVecLength { expected: usize, found: usize },
}

impl InvalidWharfMessage {
  /// Convert this [`InvalidWharfMessage`] error into a generic [`enum@Error`].
  /// A `MessageType` type must be provided in order to add context to the error.
  pub fn into_error<MessageType>(self) -> Error {
    InvalidWharfBinary::InvalidMessage {
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
}

// This will never be called. It is added in order to satisfy the compiler
// until the never type is stabilized
impl From<Infallible> for Error {
  fn from(value: Infallible) -> Self {
    match value {}
  }
}
