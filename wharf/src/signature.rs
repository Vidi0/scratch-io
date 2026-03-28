pub mod repair;
pub mod verify;

use crate::common::{MAGIC_SIGNATURE, check_magic_bytes, decompress_stream};
use crate::protos;
use crate::protos::{decode_protobuf, skip_protobuf};

use md5::Md5;
use md5::digest::{self, typenum::Unsigned};
use std::io::{BufRead, Read};

pub type Md5HashSize = <Md5 as digest::OutputSizeUser>::OutputSize;
pub const MD5_HASH_LENGTH: usize = Md5HashSize::USIZE;

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
  #[must_use]
  pub const fn total_blocks(&self) -> u64 {
    self.total_blocks
  }
}

impl<R> BlockHashIter<R>
where
  R: Read,
{
  pub fn dump_stdout(&mut self) -> Result<(), String> {
    for op in self {
      println!("{:?}", op?);
    }

    Ok(())
  }

  pub fn skip_blocks(&mut self, blocks_to_skip: u64) -> Result<(), String> {
    for _ in 0..blocks_to_skip {
      skip_protobuf(&mut self.reader)?;
    }

    self.blocks_read += blocks_to_skip;

    Ok(())
  }
}

/// <https://itch.io/docs/wharf/appendix/hashes.html>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockHash {
  /// The weak hashing algorithm used in the original RSync paper
  pub weak_hash: u32,
  /// MD5 as the "strong" hashing algorithm
  pub strong_hash: [u8; MD5_HASH_LENGTH],
}

impl TryFrom<protos::BlockHash> for BlockHash {
  type Error = String;

  fn try_from(value: protos::BlockHash) -> Result<Self, Self::Error> {
    Ok(Self {
      weak_hash: value.weak_hash,
      strong_hash: value.strong_hash.try_into().map_err(|e| {
        format!("Failed to parse strong_hash BlockHash proto message into an array!\n{e:?}")
      })?,
    })
  }
}

impl<R> Iterator for BlockHashIter<R>
where
  R: Read,
{
  type Item = Result<BlockHash, String>;

  fn next(&mut self) -> Option<Self::Item> {
    if self.blocks_read == self.total_blocks {
      return None;
    }

    self.blocks_read += 1;

    let block_hash = match decode_protobuf::<protos::BlockHash>(&mut self.reader) {
      Ok(hash) => hash,
      Err(e) => {
        return Some(Err(format!(
          "Couldn't decode BlockHash message from reader!\n{e}"
        )));
      }
    };

    Some(block_hash.try_into())
  }
}

/// Represents a decoded wharf signature file
///
/// <https://docs.itch.zone/wharf/master/file-formats/signatures.html>
///
/// Contains the header, the container describing the files/dirs/symlinks,
/// and an iterator over the signature block hashes. The iterator reads
/// from the underlying stream on the fly as items are requested.
pub struct Signature<'a> {
  pub header: protos::SignatureHeader,
  pub container_new: protos::Container,
  pub block_hash_iter: BlockHashIter<Box<dyn BufRead + 'a>>,
}

impl<'a> Signature<'a> {
  /// Dump the signature contents to standard output
  ///
  /// This prints the header, container metadata, and all block hash operations
  /// for inspection by a human reader. The internal block hash iterator is
  /// consumed during this call.
  pub fn dump_stdout(&mut self) -> Result<(), String> {
    // Print the header
    println!("{:?}", self.header);

    // Print the container
    println!("\n--- START CONTAINER INFO ---\n");
    self.container_new.dump_stdout();
    println!("\n--- END CONTAINER INFO ---");

    // Print the hashes
    println!("--- START HASH BLOCKS ---\n");
    self.block_hash_iter.dump_stdout()?;
    println!("\n--- END HASH BLOCKS ---");

    Ok(())
  }

  /// Print a concise summary of the signature to standard output
  ///
  /// Shows the compression settings and basic statistics of the
  /// new container (size, number of files, directories, and symlinks).
  pub fn print_summary(&self) {
    // Print the kind of binary
    println!(
      "wharf signature file ({})",
      // If the Signature was read using Signature::read or Signature::read_without_magic,
      // then the compression field MUST be Some, because otherwise reading would have failed
      self.header.compression.unwrap()
    );

    // Print the new container stats
    self.container_new.print_summary("new");
  }

  /// Decode a binary wharf signature assuming the magic bytes
  /// have already been consumed from the input stream
  ///
  /// For more information, see [`Signature::read`].
  pub fn read_without_magic(reader: &'a mut impl BufRead) -> Result<Self, String> {
    // Decode the signature header
    let header = decode_protobuf::<protos::SignatureHeader>(reader)?;

    // Decompress the remaining stream
    let compression_algorithm = header
      .compression
      .ok_or("Missing compressing field in Signature Header!")?
      .algorithm();

    let mut decompressed = decompress_stream(reader, compression_algorithm)?;

    // Decode the container
    let container_new = decode_protobuf::<protos::Container>(&mut decompressed)?;

    // Decode the hashes
    let block_hash_iter = BlockHashIter {
      reader: decompressed,
      total_blocks: container_new.file_blocks(),
      blocks_read: 0,
    };

    Ok(Signature {
      header,
      container_new,
      block_hash_iter,
    })
  }

  /// Decode a binary wharf signature
  ///
  /// If the magic bytes have already been read, use [`Signature::read_without_magic`].
  ///
  /// # References
  ///
  /// <https://docs.itch.zone/wharf/master/file-formats/signatures.html>
  ///
  /// <https://github.com/Vidi0/scratch-io/blob/main/docs/wharf/patch.md>
  pub fn read(reader: &'a mut impl BufRead) -> Result<Self, String> {
    // Check the magic bytes
    check_magic_bytes(reader, MAGIC_SIGNATURE)?;

    // Decode the remaining data
    Self::read_without_magic(reader)
  }
}
