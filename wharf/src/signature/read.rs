use crate::common::{check_magic_bytes, decompress_stream, file_blocks};
use crate::patch::read::PATCH_MAGIC;
use crate::protos::*;

use std::io::{BufRead, Read};

/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L17>
pub const SIGNATURE_MAGIC: u32 = PATCH_MAGIC + 1;

/// Represents a decoded wharf signature file
///
/// <https://docs.itch.zone/wharf/master/file-formats/signatures.html>
///
/// Contains the header, the container describing the files/dirs/symlinks,
/// and an iterator over the signature block hashes. The iterator reads
/// from the underlying stream on the fly as items are requested.
pub struct Signature<'a> {
  pub header: pwr::SignatureHeader,
  pub container_new: tlc::Container,
  pub block_hash_iter: BlockHashIter<Box<dyn BufRead + 'a>>,
}

/// Iterator over independent, sequential length-delimited hash messages in a [`std::io::Read`] stream
///
/// Each message is of the same type, independent and follows directly after the previous one in the stream.
/// The messages are read and decoded one by one, without loading the entire stream into memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockHashIter<R> {
  reader: R,
  total_blocks: u64,
  blocks_read: u64,
}

impl<R> BlockHashIter<R> {
  pub const fn total_blocks(&self) -> u64 {
    self.total_blocks
  }
}

impl<R> BlockHashIter<R>
where
  R: Read,
{
  pub fn skip_file(&mut self, file_size: u64, blocks_read: u64) -> Result<(), String> {
    let blocks_to_skip = file_blocks(file_size) - blocks_read;

    for _ in 0..blocks_to_skip {
      skip_protobuf(&mut self.reader)?;
    }

    self.blocks_read += blocks_to_skip;

    Ok(())
  }
}

impl<R> Iterator for BlockHashIter<R>
where
  R: Read,
{
  type Item = Result<pwr::BlockHash, String>;

  fn next(&mut self) -> Option<Self::Item> {
    if self.blocks_read == self.total_blocks {
      return None;
    }

    self.blocks_read += 1;
    Some(decode_protobuf::<pwr::BlockHash>(&mut self.reader))
  }
}

impl<'a> Signature<'a> {
  /// <https://docs.itch.zone/wharf/master/file-formats/signatures.html>
  ///
  /// <https://github.com/Vidi0/scratch-io/blob/main/docs/wharf/patch.md>
  pub fn read(reader: &'a mut impl BufRead) -> Result<Self, String> {
    // Check the magic bytes
    check_magic_bytes(reader, SIGNATURE_MAGIC)?;

    // Decode the signature header
    let header = decode_protobuf::<pwr::SignatureHeader>(reader)?;

    // Decompress the remaining stream
    let compression_algorithm = header
      .compression
      .ok_or("Missing compressing field in Signature Header!")?
      .algorithm();

    let mut decompressed = decompress_stream(reader, compression_algorithm)?;

    // Decode the container
    let container_new = decode_protobuf::<tlc::Container>(&mut decompressed)?;

    // Get the number of hash blocks
    let total_blocks = container_new
      .files
      .iter()
      .fold(0, |acc, f| acc + file_blocks(f.size as u64));

    // Decode the hashes
    let block_hash_iter = BlockHashIter {
      reader: decompressed,
      total_blocks,
      blocks_read: 0,
    };

    Ok(Signature {
      header,
      container_new,
      block_hash_iter,
    })
  }
}
