use super::common::{SIGNATURE_MAGIC, check_magic_bytes, decompress_stream};
use super::{protobuf::decode_protobuf, pwr, tlc};

use std::io::BufRead;

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

/// Iterator over independent, sequential length-delimited [`pwr::BlockHash`] Protobuf messages in a [`std::io::BufRead`] stream
///
/// Each message is of the same type, independent and follows directly after the previous one in the stream.
/// The messages are read and decoded one by one, without loading the entire stream into memory.
///
/// The iterator finishes when reaching EOF
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockHashIter<R> {
  reader: R,
}

impl<R> Iterator for BlockHashIter<R>
where
  R: BufRead,
{
  type Item = Result<pwr::BlockHash, String>;

  fn next(&mut self) -> Option<Self::Item> {
    match self.reader.fill_buf() {
      // If it couldn't read from the stream, return an error
      Err(e) => Some(Err(format!("Couldn't read from reader into buffer!\n{e}"))),

      // If there isn't any data remaining, return None
      Ok([]) => None,

      // If there is data remaining, return the decoded BlockHash Protobuf message
      Ok(_) => Some(decode_protobuf::<pwr::BlockHash>(&mut self.reader)),
    }
  }
}

/// <https://docs.itch.zone/wharf/master/file-formats/signatures.html>
///
/// The signature structure is:
///
/// - [`SIGNATURE_MAGIC`]
/// - [`pwr::SignatureHeader`]
/// - decompressed stream follows:
///   - [`tlc::Container`]    (target container)
///   - repeated sequence:
///     - [`pwr::BlockHash`]
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

  // Decode the hashes
  let block_hash_iter = BlockHashIter {
    reader: decompressed,
  };

  Ok(Signature {
    header,
    container_new,
    block_hash_iter,
  })
}
