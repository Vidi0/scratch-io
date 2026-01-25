pub mod repair;
pub mod verify;

mod read;

use crate::protos::{pwr, tlc};

use std::io::BufRead;

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
  #[must_use]
  pub const fn total_blocks(&self) -> u64 {
    self.total_blocks
  }
}
