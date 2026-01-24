use crate::protos::pwr::CompressionAlgorithm;

use std::io::{BufRead, BufReader, Read};

/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L14>
pub const MAGIC_PATCH: u32 = 0x0FEF_5F00;

/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L17>
pub const MAGIC_SIGNATURE: u32 = MAGIC_PATCH + 1;

/// Read the next 4 bytes of the reader and return its little endian u32 representation
pub fn read_magic_bytes(reader: &mut impl Read) -> Result<u32, String> {
  let mut magic_bytes = [0u8; 4];
  reader
    .read_exact(&mut magic_bytes)
    .map_err(|e| format!("Couldn't read magic bytes!\n{e}"))?;

  Ok(u32::from_le_bytes(magic_bytes))
}

/// Verify that the next four bytes of the reader match the expected magic number
///
/// # Errors
///
/// If the bytes couldn't be read from the reader or the magic bytes don't match
pub fn check_magic_bytes(reader: &mut impl Read, expected_magic: u32) -> Result<(), String> {
  // Compare the magic numbers
  if read_magic_bytes(reader)? == expected_magic {
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
pub fn decompress_stream(
  reader: &mut impl BufRead,
  algorithm: CompressionAlgorithm,
) -> Result<Box<dyn BufRead + '_>, String> {
  match algorithm {
    CompressionAlgorithm::None => Ok(Box::new(reader)),

    CompressionAlgorithm::Brotli => {
      #[cfg(feature = "brotli")]
      {
        Ok(Box::new(BufReader::new(
          // Set the buffer size to zero to allow Brotli to select the correct size
          brotli::Decompressor::new(reader, 0),
        )))
      }

      #[cfg(not(feature = "brotli"))]
      {
        Err(
          "This binary was built without Brotli support. Recompile with `--features brotli` to be able to decompress the stream".to_string(),
        )
      }
    }

    CompressionAlgorithm::Gzip => {
      #[cfg(feature = "gzip")]
      {
        Ok(Box::new(BufReader::new(flate2::bufread::GzDecoder::new(
          reader,
        ))))
      }

      #[cfg(not(feature = "gzip"))]
      {
        Err(
          "This binary was built without gzip support. Recompile with `--features gzip` to be able to decompress the stream".to_string(),
        )
      }
    }
    CompressionAlgorithm::Zstd => {
      #[cfg(feature = "zstd")]
      {
        Ok(Box::new(BufReader::new(
          zstd::Decoder::with_buffer(reader)
            .map_err(|e| format!("Couldn't create zstd decoder!\n{e}"))?,
        )))
      }

      #[cfg(not(feature = "zstd"))]
      {
        Err(
          "This binary was built without Zstd support. Recompile with `--features zstd` to be able to decompress the stream".to_string(),
        )
      }
    }
  }
}
