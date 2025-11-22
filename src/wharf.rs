#[allow(dead_code)]
mod pwr;
#[allow(dead_code)]
mod tlc;

use std::io::{BufRead, Read};

// https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L14
const PATCH_MAGIC: u32 = 0x0FEF5F00;
const SIGNATURE_MAGIC: u32 = PATCH_MAGIC + 1;

/// Iterator over independent, sequential length-delimited Protobuf messages in a `BufRead` stream
///
/// Each message is of the same type, independent and follows directly after the previous one in the stream.
/// The messages are read and decoded one by one, without loading the entire stream into memory.
struct ProtobufMessageIter<'a, R, T> {
  reader: &'a mut R,
  phantom: std::marker::PhantomData<T>,
}

impl<'a, R, T> Iterator for ProtobufMessageIter<'a, R, T>
where
  R: BufRead,
  T: prost::Message + Default,
{
  type Item = Result<T, String>;

  fn next(&mut self) -> Option<Self::Item> {
    match self.reader.fill_buf() {
      // If it couldn't read from the stream, return an error
      Err(e) => Some(Err(format!("Couldn't read from reader into buffer!\n{e}"))),

      // If there isn't any data remaining, return None
      Ok([]) => None,

      // If there is data remaining, return the decoded Protobuf message
      Ok(_) => Some(decode_protobuf::<T>(&mut self.reader)),
    }
  }
}

/// Read a Protobuf length delimiter encoded as a variable-width integer and consume its bytes
///
/// <https://protobuf.dev/programming-guides/encoding/#varints>
///
/// # Errors
///
/// If the read operation from the buffer fails, an unexpected EOF is encountered, or the length delimiter is invalid
fn read_length_delimiter(reader: &mut impl Read) -> Result<usize, String> {
  // A Protobuf varint must be 10 bytes or less
  let mut varint = [0u8; 10];

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
fn decode_protobuf<T: prost::Message + Default>(reader: &mut impl Read) -> Result<T, String> {
  let length = read_length_delimiter(reader)?;

  let mut bytes = vec![0u8; length];
  reader
    .read_exact(&mut bytes)
    .map_err(|e| format!("Couldn't read from reader into buffer!\n{e}"))?;

  T::decode(bytes.as_slice()).map_err(|e| format!("Couldn't decode Protobuf message!\n{e}"))
}

/// Create an iterator over all remaining length-delimited Protobuf messages from a `BufRead` stream
///
/// Each message is decoded independently and sequentially. The reader is advanced
/// as each message is read, without loading the entire stream into memory.
///
/// # Returns
///
/// An iterator that yields `Result<T, String>` for each decoded Protobuf message.
fn decode_protobuf_stream<T: prost::Message + Default>(
  reader: &'_ mut impl BufRead,
) -> ProtobufMessageIter<'_, impl BufRead, T> {
  ProtobufMessageIter {
    reader,
    phantom: std::marker::PhantomData,
  }
}

/// Verify that the next four bytes of the reader match the expected magic number
///
/// # Errors
///
/// If the bytes couldn't be read from the reader or the magic bytes don't match
fn check_magic_bytes(reader: &mut impl Read, expected_magic: u32) -> Result<(), String> {
  // Read the magic bytes
  let mut magic_bytes = [0u8; _];
  reader
    .read_exact(&mut magic_bytes)
    .map_err(|e| format!("Couldn't read magic bytes!\n{e}"))?;

  // Compare the magic numbers
  let actual_magic = u32::from_le_bytes(magic_bytes);
  if actual_magic == expected_magic {
    Ok(())
  } else {
    Err("The magic bytes don't match! The binary file is corrupted!".to_string())
  }
}

/// Decompress a stream using the specified decompression algorithm
///
/// # Returns
///
/// The decompressed buffered stream
///
/// # Errors
///
///
fn decompress_stream(
  reader: &mut impl BufRead,
  algorithm: pwr::CompressionAlgorithm,
) -> Result<Box<dyn std::io::BufRead + '_>, String> {
  match algorithm {
    pwr::CompressionAlgorithm::None => Ok(Box::new(reader)),

    pwr::CompressionAlgorithm::Brotli => {
      #[cfg(feature = "brotli")]
      {
        Ok(Box::new(std::io::BufReader::new(
          // Set the buffer size to zero to allow Brotli to select the correct size
          brotli::Decompressor::new(reader, 0),
        )))
      }

      #[cfg(not(feature = "brotli"))]
      {
        Err(
          "This binary was built without Brotli support. Recompile with `--features brotli` to be able to decompress the stream",
        )
      }
    }

    pwr::CompressionAlgorithm::Gzip => {
      #[cfg(feature = "gzip")]
      {
        Ok(Box::new(std::io::BufReader::new(
          flate2::read::GzDecoder::new(reader),
        )))
      }

      #[cfg(not(feature = "gzip"))]
      {
        Err(
          "This binary was built without gzip support. Recompile with `--features gzip` to be able to decompress the stream",
        )
      }
    }
    pwr::CompressionAlgorithm::Zstd => {
      #[cfg(feature = "zstd")]
      {
        Ok(Box::new(std::io::BufReader::new(
          zstd::Decoder::new(reader).map_err(|e| format!("Couldn't create zstd decoder!\n{e}"))?,
        )))
      }

      #[cfg(not(feature = "zstd"))]
      {
        Err(
          "This binary was built without Zstd support. Recompile with `--features zstd` to be able to decompress the stream",
        )
      }
    }
  }
}

pub fn read_signature(reader: &mut impl BufRead) -> Result<(), String> {
  // Check the magic bytes
  check_magic_bytes(reader, SIGNATURE_MAGIC)?;

  // Decode the SignatureHeader
  let signature_header = decode_protobuf::<pwr::SignatureHeader>(reader)?;

  // Decompress the remaining stream
  let compression_algorithm = signature_header
    .compression
    .ok_or("Missing compressing field in Signature Header!")?
    .algorithm();

  let mut decompressed = decompress_stream(reader, compression_algorithm)?;

  // Decode the container
  let _container = decode_protobuf::<tlc::Container>(&mut decompressed)?;

  // Decode the hashes
  let _hash_iter = decode_protobuf_stream::<pwr::BlockHash>(&mut decompressed);

  Ok(())
}
