//! Stream decompression for wharf binary formats.
//!
//! Wharf binaries compress their payload using one of four algorithms: none, Brotli,
//! gzip, or Zstd. This module provides [`Decompressor`], which wraps a [`BufRead`]
//! reader and transparently decompresses it using whichever algorithm is specified
//! in the binary's header.

use crate::errors::{IoError, Result};
use crate::protos::CompressionAlgorithm;

use std::io::{BufRead, Read};

/// A transparent decompressor for wharf binary streams.
///
/// Wraps a [`BufRead`] reader and decompresses it on the fly using the algorithm
/// specified in the wharf binary header. The [`None`](Decompressor::None) variant
/// passes the reader through unchanged.
///
/// Implements [`Read`], delegating to the inner decompressor for each variant.
pub enum Decompressor<'a, R: BufRead> {
  None(&'a mut R),

  // The decompressors are boxed because they are very large
  Brotli(Box<brotli::Decompressor<&'a mut R>>),
  Gzip(Box<flate2::bufread::GzDecoder<&'a mut R>>),
  Zstd(Box<zstd::Decoder<'a, &'a mut R>>),
}

impl<'a, R: BufRead> Decompressor<'a, R> {
  /// Decompress a stream using the specified decompression algorithm
  ///
  /// # Returns
  ///
  /// The decompressed stream
  pub fn new(reader: &'a mut R, algorithm: CompressionAlgorithm) -> Result<Self> {
    Ok(match algorithm {
      CompressionAlgorithm::None => {
        // Don't wrap the reader if there is no compression
        Self::None(reader)
      }
      CompressionAlgorithm::Brotli => {
        // Brotli decompression
        // Set the buffer size to zero to allow Brotli to select the correct size
        Self::Brotli(Box::new(brotli::Decompressor::new(reader, 0)))
      }
      CompressionAlgorithm::Gzip => {
        // Gzip decompression
        Self::Gzip(Box::new(flate2::bufread::GzDecoder::new(reader)))
      }
      CompressionAlgorithm::Zstd => {
        // Zstd decompression
        Self::Zstd(Box::new(
          zstd::stream::Decoder::with_buffer(reader).map_err(IoError::CreateZstdDecoderFailed)?,
        ))
      }
    })
  }
}

impl<R: BufRead> Read for Decompressor<'_, R> {
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    match self {
      Self::None(r) => r.read(buf),
      Self::Brotli(r) => r.read(buf),
      Self::Gzip(r) => r.read(buf),
      Self::Zstd(r) => r.read(buf),
    }
  }

  fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
    match self {
      Self::None(r) => r.read_exact(buf),
      Self::Brotli(r) => r.read_exact(buf),
      Self::Gzip(r) => r.read_exact(buf),
      Self::Zstd(r) => r.read_exact(buf),
    }
  }

  fn read_to_end(&mut self, buf: &mut Vec<u8>) -> std::io::Result<usize> {
    match self {
      Self::None(r) => r.read_to_end(buf),
      Self::Brotli(r) => r.read_to_end(buf),
      Self::Gzip(r) => r.read_to_end(buf),
      Self::Zstd(r) => r.read_to_end(buf),
    }
  }

  fn read_to_string(&mut self, buf: &mut String) -> std::io::Result<usize> {
    match self {
      Self::None(r) => r.read_to_string(buf),
      Self::Brotli(r) => r.read_to_string(buf),
      Self::Gzip(r) => r.read_to_string(buf),
      Self::Zstd(r) => r.read_to_string(buf),
    }
  }

  fn read_vectored(&mut self, bufs: &mut [std::io::IoSliceMut<'_>]) -> std::io::Result<usize> {
    match self {
      Self::None(r) => r.read_vectored(bufs),
      Self::Brotli(r) => r.read_vectored(bufs),
      Self::Gzip(r) => r.read_vectored(bufs),
      Self::Zstd(r) => r.read_vectored(bufs),
    }
  }
}
