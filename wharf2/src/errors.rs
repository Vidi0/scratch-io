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
  UnexpectedEOF(#[source] io::Error),

  #[error("invalid protobuf length delimiter: {length_delimiter:?}")]
  InvalidLengthDelimiter { length_delimiter: Box<[u8]> },

  #[error(
    "invalid protobuf message of type \"{message_type}\": {decode_error}
{bytes:?}"
  )]
  InvalidProtoMessage {
    message_type: &'static str,
    decode_error: String,
    bytes: Box<[u8]>,
  },
}

#[derive(Debug, Error)]
pub enum IoError {
  #[error("failed to read the wharf binary data: {0}")]
  WharfBinaryReadFailed(#[source] io::Error),
}
