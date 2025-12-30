use crate::common::{BLOCK_SIZE, SIGNATURE_MAGIC, check_magic_bytes, decompress_stream};
use crate::protos::*;

use std::io::{BufRead, Read};

/// Represents a decoded wharf signature file
///
/// <https://docs.itch.zone/wharf/master/file-formats/signatures.html>
///
/// Contains the header, the container describing the files/dirs/symlinks,
/// and an iterator over the signature block hashes. The iterator reads
/// from the underlying stream on the fly as items are requested.
#[derive(Debug, Clone, PartialEq)]
pub struct Signature<R> {
  pub header: pwr::SignatureHeader,
  pub container_new: tlc::Container,
  pub block_hash_iter: BlockHashIter<R>,
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
  pub fn total_blocks(&self) -> u64 {
    self.total_blocks
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

/// <https://docs.itch.zone/wharf/master/file-formats/signatures.html>
///
/// <https://github.com/Vidi0/scratch-io/blob/main/docs/wharf/patch.md>
pub fn read_signature(reader: &mut impl BufRead) -> Result<Signature<impl BufRead>, String> {
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
  let total_blocks = container_new.files.iter().fold(0, |acc, f| {
    // For each file, compute how many blocks it occupies
    // If the file is empty, still count one block for its empty hash
    acc + (f.size as u64).div_ceil(BLOCK_SIZE).max(1)
  });

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
