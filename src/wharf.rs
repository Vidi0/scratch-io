#[allow(dead_code)]
mod protos;

use prost::Message;
use std::io::BufRead;

// https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L14
const PATCH_MAGIC: u32 = 0x0FEF5F00;
const SIGNATURE_MAGIC: u32 = PATCH_MAGIC + 1;

/// Read a protobuf varint (variable-width integers) and consume its bytes
///
/// <https://protobuf.dev/programming-guides/encoding/#varints>
///
/// # Errors
///
/// If the read operation from the buffer fails, an unexpected EOF is encountered, or the varint is invalid
fn read_varint(reader: &mut impl BufRead) -> Result<usize, String> {
  // A protobuf varint must be 10 bytes or less
  let mut varint: Vec<u8> = Vec::with_capacity(10);

  loop {
    // Get the next chunk
    let chunk = reader
      .fill_buf()
      .map_err(|e| format!("Couldn't read from reader into buffer!\n{e}"))?;

    // Read one byte
    if chunk.is_empty() {
      return Err("Unexpected EOF while reading varint".to_string());
    }

    let byte = chunk[0];
    varint.push(byte);
    reader.consume(1);

    // The most significant bit indicates whether there are more bytes in the varint
    if (byte & 0x80) == 0 {
      break;
    }
  }

  // Decode the varint
  prost::decode_length_delimiter(&varint[..])
    .map_err(|e| format!("Couldn't decode the signature header length delimiter!\n{e}"))
}

pub fn read_signature(reader: &mut impl BufRead) -> Result<(), String> {
  // Check the magic bytes
  let mut magic_bytes = [0u8; 4];
  reader
    .read_exact(&mut magic_bytes)
    .map_err(|e| format!("Couldn't read magic bytes!\n{e}"))?;

  let magic = u32::from_le_bytes(magic_bytes);
  if magic != SIGNATURE_MAGIC {
    return Err("The magic bytes don't match! The signature is corrupted!".to_string());
  }

  // Decore the SignatureHeader
  let signature_header_length = read_varint(reader)?;
  let mut signature_header_bytes = vec![0u8; signature_header_length];
  reader
    .read_exact(&mut signature_header_bytes)
    .map_err(|e| format!("Couldn't read from reader into buffer!\n{e}"))?;

  let _signature_header = protos::SignatureHeader::decode(signature_header_bytes.as_slice())
    .map_err(|e| format!("Couldn't decode Signature Header!\n{e}"))?;

  Ok(())
}
