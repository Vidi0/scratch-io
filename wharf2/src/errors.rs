use std::convert::Infallible;
use std::io;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

/// Top-level error type for all wharf operations
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

/// Errors that arise when a wharf binary is structurally or semantically invalid.
///
/// Variants are ordered by parsing stage: stream-level errors first,
/// then protobuf decoding, then field conversion, then cross-field validation.
#[derive(Debug, Error)]
pub enum InvalidBinary {
  /// The stream ended before all expected bytes were read
  #[error("expected more bniary data, got EOF")]
  UnexpectedEOF,

  /// The magic bytes at the start of the stream did not match the expected value
  #[error("magic bytes mismatch: expected {expected}, found {found}")]
  MagicMismatch { expected: u32, found: u32 },

  /// The magic bytes did not match any known wharf binary format
  #[error("magic bytes mismatch: does not match any known wharf binary format: {found}")]
  MagicNotFound { found: u32 },

  /// The protobuf length delimiter preceding a message could not be decoded
  #[error("invalid protobuf length delimiter: {length_delimiter:?}")]
  InvalidLengthDelimiter { length_delimiter: Box<[u8]> },

  /// The raw bytes of a protobuf message failed to be decoded
  #[error(
    "could not decode protobuf message of type \"{message_type}\":
invalid protobuf message: {decode_error}
{bytes:?}"
  )]
  InvalidProto {
    message_type: &'static str,
    decode_error: String,
    bytes: Box<[u8]>,
  },

  /// A protobuf message was decoded successfully, but one of its fields
  /// could not be converted into the expected intermediate representation
  #[error(
    "could not parse protobuf message of type \"{message_type}\":
failed to parse field \"{field_name}\":
{source}"
  )]
  UnparseableField {
    message_type: &'static str,
    field_name: &'static str,
    source: UnparseableField,
  },

  /// A protobuf message was decoded and its fields individually parsed,
  /// but the message is inconsistent with respect to other data seen so far
  #[error(
    "inconsistent data in message of type \"{message_type}\":
{source}"
  )]
  InconsistentMessage {
    message_type: &'static str,
    source: InconsistentMessage,
  },
}

/// Errors that occur when converting a decoded protobuf field
/// into its expected intermediate representation
#[derive(Debug, Error)]
pub enum UnparseableField {
  /// A required field was absent in the protobuf message
  #[error("missing field")]
  MissingField,

  /// A field expected to be a non-negative integer representable as `usize`
  /// contained a value that does not satisfy that constraint
  #[error("expected valid usize, found: {int}")]
  ExpectedUsize { int: i64 },

  /// A field expected to be a non-negative integer representable as `u64`
  /// contained a value that does not satisfy that constraint
  #[error("expected valid u64, found: {int}")]
  ExpectedU64 { int: i64 },

  /// A repeated field had a length that did not match the expected length
  #[error("expected vector length of {expected}, found length {found}")]
  ExpectedVecLength { expected: usize, found: usize },
}

impl UnparseableField {
  /// Convert this [`UnparseableField`] error into a generic [`enum@Error`].
  ///
  /// `MessageType` provides the name of the proto message type that contained
  /// the invalid field. `field_name` identifies which field failed conversion.
  pub fn into_error<MessageType>(self, field_name: &'static str) -> Error {
    InvalidBinary::UnparseableField {
      message_type: std::any::type_name::<MessageType>(),
      field_name,
      source: self,
    }
    .into()
  }
}

/// Errors that occur when a decoded message is inconsistent with
/// respect to the surrounding message stream or other already-parsed data
#[derive(Debug, Error)]
pub enum InconsistentMessage {
  #[error(
    "expected rsync SyncOp with type=HeyYouDidIt after bsdiff Control operation with eof=true,\
but found SyncOp with a different type instead"
  )]
  ExpectedHeyYouDidIt,

  #[error(
    "expected consecutive patch SyncHeader file_index
expected index: {expected}, found: {found}"
  )]
  NonconsecutivePatchHeaderIndex { expected: usize, found: usize },

  #[error(
    "out of bounds file index
container file count: {container_file_count}, found index: {file_index}"
  )]
  OutOfBoundsFileIndex {
    container_file_count: usize,
    file_index: usize,
  },

  #[error(
    "overflowing block span
block index: {block_index}, block span: {block_span}"
  )]
  OverflowingBlockSpan { block_index: u64, block_span: u64 },
}

impl InconsistentMessage {
  /// Convert this [`InconsistentMessage`] error into a generic [`enum@Error`].
  ///
  /// `MessageType` provides the name of the proto message type in which
  /// the inconsistency was detected.
  pub fn into_error<MessageType>(self) -> Error {
    InvalidBinary::InconsistentMessage {
      message_type: std::any::type_name::<MessageType>(),
      source: self,
    }
    .into()
  }
}

/// Errors that arise from underlying I/O operations
#[derive(Debug, Error)]
pub enum IoError {
  /// Reading bytes from the wharf binary stream failed
  #[error("failed to read the wharf binary data: {0}")]
  WharfBinaryReadFailed(#[source] io::Error),

  /// Creating a Zstandard decoder over the stream failed
  #[error("failed to create a new Zstandard decoder: {0}")]
  CreateZstdDecoderFailed(#[source] io::Error),

  /// Writing to the dump output failed
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
