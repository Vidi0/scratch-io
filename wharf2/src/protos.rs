//! Protobuf message decoding for wharf binary streams.
//!
//! This module provides the [`Message`] trait, which is implemented by types that
//! can be decoded from length-delimited Protobuf messages in a wharf binary stream.
//! Each message is preceded by a varint length delimiter, as described in the
//! [Protobuf encoding specification](https://protobuf.dev/programming-guides/encoding/#length-types).

mod definitions;

pub use definitions::*;

use crate::binaries::{Dump, read_wharf_exact};
use crate::errors::{Error, InvalidWharfBinary, InvalidWharfMessage, IoError, Result};

use std::fmt::Debug;
use std::hash::Hash;
use std::io::{self, Read, Write};

/// <https://protobuf.dev/programming-guides/encoding/#varints>
const PROTOBUF_VARINT_MAX_LENGTH: usize = 10;

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

/// A type that can be decoded from a length-delimited Protobuf message in a wharf
/// binary stream.
///
/// Each implementing type has an associated [`Message::ProtoMessage`] type that handles
/// the raw Protobuf decoding, and a [`TryFrom`] conversion that maps it into the
/// corresponding Rust type. [`Message::decode`] and [`Message::skip`] operate on any reader,
/// advancing it by exactly one message per call.
pub trait Message
where
  Self: Sized,
  Self: Clone + Debug + PartialEq + Eq + Hash,
  Self::ProtoMessage: TryInto<Self>,
  <Self::ProtoMessage as TryInto<Self>>::Error: Into<Error>,
{
  type ProtoMessage: Default + prost::Message;

  /// Decode a length-delimited Protobuf message and advance the reader.
  ///
  /// # Returns
  ///
  /// The deserialized Protobuf message
  ///
  /// # Errors
  ///
  /// If the reader failed to read the message, or if the Protobuf message is invalid
  fn decode(reader: &mut impl Read) -> Result<Self> {
    use prost::Message;

    // Decode the length delimiter
    let length = read_length_delimiter(reader)?;

    // Read the bytes into a buffer
    let mut bytes = vec![0u8; length];
    read_wharf_exact(reader, &mut bytes)?;

    // Decode the protobuf message
    let proto = Self::ProtoMessage::decode(bytes.as_slice()).map_err(|e| {
      InvalidWharfMessage::InvalidProtoMessage {
        decode_error: e.to_string(),
        bytes: bytes.into_boxed_slice(),
      }
      .into_error::<Self>()
    })?;

    // Parse the protobuf message
    proto.try_into().map_err(|e| e.into())
  }

  /// Advance the reader to the end of the next length-delimited Protobuf message.
  ///
  /// # Errors
  ///
  /// If the reader failed to be advanced
  fn skip(reader: &mut impl Read) -> Result<()> {
    // Decode the length delimiter
    let length = read_length_delimiter(reader)? as u64;

    // Read the bytes into the void
    let read_bytes = io::copy(&mut reader.take(length), &mut io::empty())
      .map_err(IoError::WharfBinaryReadFailed)?;

    // Check the number of read bytes matches the expected amount
    if length == read_bytes {
      Ok(())
    } else {
      Err(InvalidWharfBinary::UnexpectedEOF.into())
    }
  }
}

impl<T: Message> Dump for T {
  fn dump(&mut self, writer: &mut impl Write) -> Result<()> {
    writeln!(writer, "{:?}", self).map_err(IoError::WriteDumpFailed)?;
    Ok(())
  }
}
