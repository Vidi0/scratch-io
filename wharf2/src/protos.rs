/// <https://github.com/itchio/wharf/blob/5e5efc838cdbaee7915246d5102af78a3a31e74d/bsdiff/bsdiff.proto>
///
/// More information about bsdiff wharf patches:
/// <https://web.archive.org/web/20211123032456/https://twitter.com/fasterthanlime/status/790617515009437701>
mod bsdiff;
/// <https://github.com/itchio/wharf/blob/5e5efc838cdbaee7915246d5102af78a3a31e74d/pwr/pwr.proto>
mod pwr;
/// <https://github.com/itchio/lake/blob/d93a9d33bb65f76200e07d9606e1e251fd09cb07/tlc/tlc.proto>
mod tlc;

pub use bsdiff::*;
pub use pwr::*;
pub use tlc::*;

use crate::errors::{InvalidWharfBinary, IoError, Result};

use std::io::{self, Read};

/// <https://protobuf.dev/programming-guides/encoding/#varints>
const PROTOBUF_VARINT_MAX_LENGTH: usize = 10;

/// Reads the exact number of bytes required to fill `buf`.
///
/// Maps the error to return an [`InvalidWharfBinary::UnexpectedEOF`] error
/// if an unexpected EOF was encountered, because calling this function means
/// data was expected from the wharf binary.
fn read_wharf_exact(reader: &mut impl Read, buf: &mut [u8]) -> Result<()> {
  reader.read_exact(buf).map_err(|e| {
    // Return an InvalidWharfBinary error if an unexpected EOF is encountered,
    // and an IO error for every other case.
    if let io::ErrorKind::UnexpectedEof = e.kind() {
      InvalidWharfBinary::UnexpectedEOF(e).into()
    } else {
      IoError::WharfBinaryReadFailed(e).into()
    }
  })
}

/// Read a Protobuf length delimiter encoded as a variable-width integer and consume its bytes
///
/// <https://protobuf.dev/programming-guides/encoding/#length-types>
///
/// <https://protobuf.dev/programming-guides/encoding/#varints>
///
/// # Errors
///
/// If the read operation from the buffer fails, an unexpected EOF is encountered, or the length delimiter is invalid
fn read_length_delimiter(reader: &mut impl Read) -> Result<usize> {
  // A Protobuf varint must be 10 bytes or less
  let mut varint = [0u8; PROTOBUF_VARINT_MAX_LENGTH];

  for current_byte in &mut varint {
    // Read one byte
    read_wharf_exact(reader, std::slice::from_mut(current_byte))?;

    // The most significant bit indicates whether there are more bytes in the varint
    if (*current_byte & 0x80) == 0 {
      break;
    }
  }

  // Decode the varint
  prost::decode_length_delimiter(varint.as_slice()).map_err(|_| {
    InvalidWharfBinary::InvalidLengthDelimiter {
      length_delimiter: Box::new(varint),
    }
    .into()
  })
}

/// Decode a length-delimited Protobuf message
///
/// Advance the reader to the end of the message
///
/// # Returns
///
/// The deserialized Protobuf message
///
/// # Errors
///
/// If the reader could not be read, or if the Protobuf message is invalid
pub fn decode_protobuf<T: prost::Message + Default>(reader: &mut impl Read) -> Result<T> {
  let length = read_length_delimiter(reader)?;

  let mut bytes = vec![0u8; length];
  read_wharf_exact(reader, &mut bytes)?;

  T::decode(bytes.as_slice()).map_err(|e| {
    InvalidWharfBinary::InvalidProtoMessage {
      message_type: std::any::type_name::<T>(),
      decode_error: e.to_string(),
      bytes: bytes.into_boxed_slice(),
    }
    .into()
  })
}

/// Skip the next length-delimited Protobuf message
///
/// Advance the reader to the end of the message
///
/// # Errors
///
/// If the reader could not be read
pub fn skip_protobuf(reader: &mut impl Read) -> Result<()> {
  let length = read_length_delimiter(reader)?;

  std::io::copy(&mut reader.take(length as u64), &mut io::empty())
    .map(|_| ())
    .map_err(|e| IoError::WharfBinaryReadFailed(e).into())
}
