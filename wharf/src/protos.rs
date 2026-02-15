/// <https://github.com/itchio/wharf/blob/5e5efc838cdbaee7915246d5102af78a3a31e74d/bsdiff/bsdiff.proto>
///
/// More information about bsdiff wharf patches:
/// <https://web.archive.org/web/20211123032456/https://twitter.com/fasterthanlime/status/790617515009437701>
pub mod bsdiff;
/// <https://github.com/itchio/wharf/blob/5e5efc838cdbaee7915246d5102af78a3a31e74d/pwr/pwr.proto>
pub mod pwr;
/// <https://github.com/itchio/lake/blob/d93a9d33bb65f76200e07d9606e1e251fd09cb07/tlc/tlc.proto>
pub mod tlc;

use std::io::Read;

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
fn read_length_delimiter(reader: &mut impl Read) -> Result<usize, String> {
  // A Protobuf varint must be 10 bytes or less
  let mut varint = [0u8; PROTOBUF_VARINT_MAX_LENGTH];

  for current_byte in &mut varint {
    // Read one byte
    let mut byte = [0u8; 1];
    reader
      .read_exact(&mut byte)
      .map_err(|e| format!("Couldn't read from reader into buffer!\n{e}"))?;

    // Save the byte in the array
    *current_byte = byte[0];

    // The most significant bit indicates whether there are more bytes in the varint
    if (byte[0] & 0x80) == 0 {
      break;
    }
  }

  // Decode the varint
  prost::decode_length_delimiter(varint.as_slice())
    .map_err(|e| format!("Couldn't decode the signature header length delimiter!\n{e}"))
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
pub(crate) fn decode_protobuf<T: prost::Message + Default>(
  reader: &mut impl Read,
) -> Result<T, String> {
  let length = read_length_delimiter(reader)?;

  let mut bytes = vec![0u8; length];
  reader
    .read_exact(&mut bytes)
    .map_err(|e| format!("Couldn't read from reader into buffer!\n{e}"))?;

  T::decode(bytes.as_slice()).map_err(|e| format!("Couldn't decode Protobuf message!\n{e}"))
}

/// Skip the next length-delimited Protobuf message
///
/// Advance the reader to the end of the message
///
/// # Errors
///
/// If the reader could not be read
pub(crate) fn skip_protobuf(reader: &mut impl Read) -> Result<(), String> {
  let length = read_length_delimiter(reader)?;

  std::io::copy(&mut reader.take(length as u64), &mut std::io::sink())
    .map(|_| ())
    .map_err(|e| format!("Couldn't read from reader into a sink!\n{e}"))
}
