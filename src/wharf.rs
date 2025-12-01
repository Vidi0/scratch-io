/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/bsdiff/bsdiff.proto>
///
/// More information about bsdiff wharf patches:
/// <https://web.archive.org/web/20211123032456/https://twitter.com/fasterthanlime/status/790617515009437701>
pub mod bsdiff;
/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/pwr.proto>
pub mod pwr;
/// <https://github.com/itchio/lake/blob/cc4284ec2b2a9ebc4735d7560ed8216de6ffac6f/tlc/tlc.proto>
pub mod tlc;

use md5::{Digest, Md5};
use std::io::{BufRead, Read};

/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L14>
const PATCH_MAGIC: u32 = 0x0FEF5F00;
const SIGNATURE_MAGIC: u32 = PATCH_MAGIC + 1;

/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L30>
const _MODE_MASK: u32 = 0o644;

/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L33>
const BLOCK_SIZE: usize = 64 * 1024;

/// <https://protobuf.dev/programming-guides/encoding/#varints>
const PROTOBUF_VARINT_MAX_LENGTH: usize = 10;

/// Represents a decoded wharf signature file
///
/// <https://docs.itch.ovh/wharf/master/file-formats/signatures.html>
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

/// Represents a decoded wharf patch file
///
/// <https://docs.itch.ovh/wharf/master/file-formats/patches.html>
///
/// Contains the header, the old and new containers describing file system
/// state before and after the patch, and an iterator over the patch operations.
/// The iterator reads from the underlying stream on the fly as items are requested.
#[derive(Debug, Clone, PartialEq)]
pub struct Patch<R> {
  pub header: pwr::PatchHeader,
  pub container_old: tlc::Container,
  pub container_new: tlc::Container,
  pub sync_op_iter: SyncEntryIter<R>,
}

/// Iterator over independent, sequential length-delimited [`pwr::BlockHash`] Protobuf messages in a [`std::io::BufRead`] stream
///
/// Each message is of the same type, independent and follows directly after the previous one in the stream.
/// The messages are read and decoded one by one, without loading the entire stream into memory.
///
/// The iterator finishes when reaching EOF
#[derive(Debug, Clone, PartialEq)]
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

/// A single logical unit in a wharf patch stream.
///
/// The patch stream alternates between:
///   1. A [`SyncEntry::Header`] entry describing how to patch a specific file  
///      ([`pwr::SyncHeader`], and optionally a [`pwr::BsdiffHeader`])
///   2. A sequence of [`SyncEntry::Op`] entries ([`pwr::SyncOp`]) containing the actual patch
///      operations for that file, ending with one whose type is [`pwr::sync_op::Type::HeyYouDidIt`]
///
/// After the final [`pwr::sync_op::Type::HeyYouDidIt`], the next call to the iterator yields
/// the next [`SyncEntry::Header`], or `None` if the stream has ended.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncEntry {
  /// Describes a new file patching sequence.
  ///  
  /// Contains the file-level [`pwr::SyncHeader`] and, if the header specifies a
  /// bsdiff patch, a [`pwr::BsdiffHeader`].
  Header {
    header: pwr::SyncHeader,
    bsdiff_header: Option<pwr::BsdiffHeader>,
  },

  /// A single [`pwr::SyncOp`] operation belonging to the current file.
  ///  
  /// [`pwr::SyncOp`] messages are emitted sequentially until one with the
  /// [`pwr::sync_op::Type::HeyYouDidIt`] operation type is encountered, marking the end
  /// of the current file's operation sequence.
  Op(pwr::SyncOp),
}

/// Iterator over a full wharf patch stream
///
/// The stream is structured as alternating segments:
///   - A [`pwr::SyncHeader`] (and optional [`pwr::BsdiffHeader`])
///   - Followed by a sequence of [`pwr::SyncOp`] messages
///
/// This iterator yields each header and each operation in order, without
/// loading the entire stream into memory. It continuously reads from the
/// underlying `BufRead` source and decodes length-delimited Protobuf
/// messages on demand.
///
/// Once a [`pwr::SyncOp`] with type [`pwr::sync_op::Type::HeyYouDidIt`] is read, the iterator
/// considers the current file's operation sequence finished, and the next call to
/// `next()` yields the next [`SyncEntry::Header`] if any data remains.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncEntryIter<R> {
  reader: R,

  /// Tracks whether the previous [`SyncEntry`] was a [`pwr::sync_op::Type::HeyYouDidIt`],
  /// indicating that the next call to `next()` must decode a new header instead of another
  /// [`pwr::SyncOp`].
  op_sequence_finished: bool,
}

impl<R> Iterator for SyncEntryIter<R>
where
  R: BufRead,
{
  type Item = Result<SyncEntry, String>;

  fn next(&mut self) -> Option<Self::Item> {
    // If the OP sequence finished, then decode a new header,
    // or return None if the stream reached EOF
    if self.op_sequence_finished {
      match self.reader.fill_buf() {
        // If it couldn't read from the stream, return an error
        Err(e) => Some(Err(format!("Couldn't read from reader into buffer!\n{e}"))),

        // If there isn't any data remaining, return None
        Ok([]) => None,

        // If there is data remaining, return the decoded header
        Ok(_) => {
          // Set the variable to go back to the OP cycle
          self.op_sequence_finished = false;

          // Decode the SyncHeader
          let header = match decode_protobuf::<pwr::SyncHeader>(&mut self.reader) {
            Err(e) => return Some(Err(e)),
            Ok(sync_header) => sync_header,
          };

          // Decode the BsdiffHeader (if the header type is Bsdiff)
          let bsdiff_header = match header.r#type() {
            pwr::sync_header::Type::Rsync => None,
            pwr::sync_header::Type::Bsdiff => {
              match decode_protobuf::<pwr::BsdiffHeader>(&mut self.reader) {
                Err(e) => return Some(Err(e)),
                Ok(bsdiff_header) => Some(bsdiff_header),
              }
            }
          };

          Some(Ok(SyncEntry::Header {
            header,
            bsdiff_header,
          }))
        }
      }
    }
    // If the OP sequence continues, then decode the SyncOp message
    else {
      Some(
        decode_protobuf::<pwr::SyncOp>(&mut self.reader)
          .inspect(|sync_op| {
            // If the SyncOp Type is HeyYouDidIt, then this OP sequence has finished
            if let pwr::sync_op::Type::HeyYouDidIt = sync_op.r#type() {
              self.op_sequence_finished = true;
            }
          })
          .map(SyncEntry::Op),
      )
    }
  }
}

/// Read a Protobuf length delimiter encoded as a variable-width integer and consume its bytes
///
/// <https://protobuf.dev/programming-guides/encoding/#length-types>
///
/// <https://protobuf.dev/programming-guides/encoding/#varints>
///
/// # Errors
///
/// If the read operation from the buffer fails, an unexpected EOF is encountered, or the length delimiter is invalid
fn read_length_delimiter(reader: &mut impl Read) -> Result<usize, String> {
  // A Protobuf varint must be 10 bytes or less
  let mut varint = [0u8; PROTOBUF_VARINT_MAX_LENGTH];

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
          flate2::bufread::GzDecoder::new(reader),
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
          zstd::Decoder::with_buffer(reader)
            .map_err(|e| format!("Couldn't create zstd decoder!\n{e}"))?,
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

/// <https://docs.itch.ovh/wharf/master/file-formats/signatures.html>
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

pub fn verify_files(
  build_folder: &std::path::Path,
  signature_reader: &mut impl BufRead,
) -> Result<(), String> {
  // Read the wharf signature from the reader
  let mut signature = read_signature(signature_reader)
    .map_err(|e| format!("Couldn't read signature stream!\n{e}"))?;

  // This buffer will hold the current block that is being hashed
  let mut buffer = vec![0u8; BLOCK_SIZE];

  // Loop over all the files in the signature container
  for container_file in &signature.container_new.files {
    let file_path = build_folder.join(&container_file.path);
    let file = std::fs::File::open(&file_path).map_err(|e| {
      format!(
        "Couldn't open file: \"{}\"\n{e}",
        file_path.to_string_lossy()
      )
    })?;

    // Check if the file length matches
    let metadata = file.metadata().map_err(|e| {
      format!(
        "Couldn't get file metadata: \"{}\"\n{e}",
        file_path.to_string_lossy()
      )
    })?;

    if metadata.len() as i64 != container_file.size {
      return Err(format!(
        "The signature and the in-disk size of \"{}\" don't match!",
        file_path.to_string_lossy()
      ));
    }

    // For each block in the file, compare its hash with the one provided in the signature
    let mut file_bufreader = std::io::BufReader::new(file);
    let mut block_index: usize = 0;

    loop {
      let block_start: usize = block_index * BLOCK_SIZE;
      let block_end: usize = std::cmp::min(block_start + BLOCK_SIZE, container_file.size as usize);

      // Hash the current block
      let buf = &mut buffer[..block_end - block_start];
      file_bufreader.read_exact(buf).map_err(|e| {
        format!(
          "Couldn't read file data into buffer: \"{}\"\n{e}",
          file_path.to_string_lossy()
        )
      })?;

      let hash = Md5::digest(buf);

      // Get the expected hash from the signature
      let signature_hash = signature.block_hash_iter.next().ok_or_else(|| {
        "Expected a block hash message in the signature, but EOF was encountered!".to_string()
      })??;

      // Compare the hashes
      if *signature_hash.strong_hash != *hash {
        return Err(format!(
          "Hash mismatch!
  Signature: {:X?}
  In-disk: {:X?}",
          signature_hash.strong_hash, hash,
        ));
      }

      // If the file has been fully read, proceed to the next one
      if block_end == container_file.size as usize {
        break;
      }

      block_index += 1;
    }
  }

  Ok(())
}

/// <https://docs.itch.ovh/wharf/master/file-formats/patches.html>
///
/// The patch structure is:
///
/// - [`PATCH_MAGIC`]
/// - [`pwr::PatchHeader`]
/// - decompressed stream follows:
///   - [`tlc::Container`]    (target container)
///   - [`tlc::Container`]    (source container)
///   - repeated sequence:
///     - [`pwr::SyncHeader`]
///     - Optional [`pwr::BsdiffHeader`] if the previous header type is [`pwr::sync_header::Type::Bsdiff`]
///       - repeated sequence:
///       - [`pwr::SyncOp`]
///     - [`pwr::SyncOp`] (Type = HEY_YOU_DID_IT)  // end of file’s series
pub fn read_patch(reader: &mut impl BufRead) -> Result<Patch<impl BufRead>, String> {
  // Check the magic bytes
  check_magic_bytes(reader, PATCH_MAGIC)?;

  // Decode the patch header
  let header = decode_protobuf::<pwr::PatchHeader>(reader)?;

  // Decompress the remaining stream
  let compression_algorithm = header
    .compression
    .ok_or("Missing compressing field in Patch Header!")?
    .algorithm();

  let mut decompressed = decompress_stream(reader, compression_algorithm)?;

  // Decode the containers
  let container_old = decode_protobuf::<tlc::Container>(&mut decompressed)?;
  let container_new = decode_protobuf::<tlc::Container>(&mut decompressed)?;

  // Decode the sync operations
  let sync_op_iter = SyncEntryIter {
    reader: decompressed,
    // Set op_sequence_finished to true to start decoding with a header
    op_sequence_finished: true,
  };

  Ok(Patch {
    header,
    container_old,
    container_new,
    sync_op_iter,
  })
}
