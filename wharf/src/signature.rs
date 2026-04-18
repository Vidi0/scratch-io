use crate::common::{
  BLOCK_SIZE, MAGIC_SIGNATURE, Reader, block_count, check_magic_bytes, decompress_stream,
};
use crate::protos;
use crate::protos::{decode_protobuf, skip_protobuf};

use std::io::BufRead;

pub mod strong_hash {
  pub use md5::Digest;
  pub type Hasher = md5::Md5;
  pub type Output = md5::digest::Output<Hasher>;
}

/// <https://itch.io/docs/wharf/appendix/hashes.html>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockHash {
  /// The size of this file block in the container
  pub block_size: usize,

  /// The weak hashing algorithm used in the original RSync paper
  pub weak_hash: u32,
  /// MD5 as the "strong" hashing algorithm
  pub strong_hash: strong_hash::Output,
}

impl BlockHash {
  fn try_from(value: protos::BlockHash, block_size: usize) -> Result<Self, String> {
    Ok(Self {
      block_size,
      weak_hash: value.weak_hash,
      strong_hash: value.strong_hash.as_slice().try_into().map_err(|e| {
        format!("Failed to parse strong_hash BlockHash proto message into an array!\n{e:?}")
      })?,
    })
  }
}

pub struct FileHashIter<'a, 'reader> {
  reader: &'a mut Reader<'reader>,
  remaining_blocks: &'a mut u64,
  remaining_file_size: u64,
}

impl Iterator for FileHashIter<'_, '_> {
  type Item = Result<BlockHash, String>;

  fn next(&mut self) -> Option<Self::Item> {
    if *self.remaining_blocks == 0 {
      return None;
    }

    // Get the current block size and update the internal counters
    let block_size = self.remaining_file_size.min(BLOCK_SIZE as u64);
    self.remaining_file_size -= block_size;
    *self.remaining_blocks -= 1;

    // Decode the hash message from the reader
    let block_hash = match decode_protobuf::<protos::BlockHash>(&mut self.reader) {
      Ok(hash) => hash,
      Err(e) => {
        return Some(Err(format!(
          "Couldn't decode BlockHash message from reader!\n{e}"
        )));
      }
    };

    Some(BlockHash::try_from(block_hash, block_size as usize))
  }
}

impl FileHashIter<'_, '_> {
  pub fn dump_stdout(&mut self) -> Result<(), String> {
    println!("\n{} blocks", self.remaining_blocks);

    for op in self {
      println!("{:?}", op?);
    }

    Ok(())
  }
}

/// Iterator over independent, sequential length-delimited hash messages in a [`std::io::Read`] stream
///
/// Each message is of the same type, independent and follows directly after the previous one in the stream.
/// The messages are read and decoded one by one, without loading the entire stream into memory.
pub struct BlockHashIter<'reader> {
  reader: Box<Reader<'reader>>,
  last_entry_remaining_blocks: u64,
}

impl<'reader> BlockHashIter<'reader> {
  pub fn dump_stdout(&mut self, container: &protos::Container) -> Result<(), String> {
    for file in &container.files {
      let mut file_hash = self.next_file(file.size as u64)?;
      file_hash.dump_stdout()?;
    }

    Ok(())
  }

  pub fn skip_blocks(&mut self, blocks_to_skip: u64) -> Result<(), String> {
    for _ in 0..blocks_to_skip {
      skip_protobuf(&mut self.reader)?;
    }

    Ok(())
  }

  pub fn next_file<'a>(&'a mut self, file_size: u64) -> Result<FileHashIter<'a, 'reader>, String> {
    let file_blocks = block_count(file_size);

    // Skip the blocks that were not obtained in the last file hash iter
    self.skip_blocks(self.last_entry_remaining_blocks)?;
    self.last_entry_remaining_blocks = file_blocks;

    Ok(FileHashIter {
      reader: &mut self.reader,
      remaining_blocks: &mut self.last_entry_remaining_blocks,
      remaining_file_size: file_size,
    })
  }
}

/// Represents a decoded wharf signature file
///
/// <https://docs.itch.zone/wharf/master/file-formats/signatures.html>
///
/// Contains the header, the container describing the files/dirs/symlinks,
/// and an iterator over the signature block hashes. The iterator reads
/// from the underlying stream on the fly as items are requested.
pub struct Signature<'reader> {
  pub header: protos::SignatureHeader,
  pub container_new: protos::Container,
  pub block_hash_iter: BlockHashIter<'reader>,
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
    self.block_hash_iter.dump_stdout(&self.container_new)?;
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
  pub fn read_without_magic<R>(reader: &'a mut R) -> Result<Self, String>
  where
    R: BufRead + Send,
  {
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
      last_entry_remaining_blocks: 0,
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
  pub fn read<R>(reader: &'a mut R) -> Result<Self, String>
  where
    R: BufRead + Send,
  {
    // Check the magic bytes
    check_magic_bytes(reader, MAGIC_SIGNATURE)?;

    // Decode the remaining data
    Self::read_without_magic(reader)
  }
}
