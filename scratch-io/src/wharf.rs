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
use std::io::{BufRead, Read, Seek, Write};

/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L14>
const PATCH_MAGIC: u32 = 0x0FEF_5F00;
const SIGNATURE_MAGIC: u32 = PATCH_MAGIC + 1;

/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L30>
const _MODE_MASK: u32 = 0o644;

/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L33>
const BLOCK_SIZE: usize = 64 * 1024;

/// <https://protobuf.dev/programming-guides/encoding/#varints>
const PROTOBUF_VARINT_MAX_LENGTH: usize = 10;

const MAX_OPEN_FILES_PATCH: std::num::NonZeroUsize = std::num::NonZeroUsize::new(16).unwrap();

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

/// Represents a decoded wharf patch file
///
/// <https://docs.itch.zone/wharf/master/file-formats/patches.html>
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

#[derive(Debug, PartialEq, Eq)]
pub struct RsyncOpIter<'a, R> {
  reader: &'a mut R,
}

impl<R> Iterator for RsyncOpIter<'_, R>
where
  R: BufRead,
{
  type Item = Result<pwr::SyncOp, String>;

  fn next(&mut self) -> Option<Self::Item> {
    match decode_protobuf::<pwr::SyncOp>(&mut self.reader) {
      Err(e) => Some(Err(format!(
        "Couldn't decode Rsync SyncOp message from reader!\n{e}"
      ))),

      Ok(sync_op) => {
        if sync_op.r#type() == pwr::sync_op::Type::HeyYouDidIt {
          None
        } else {
          Some(Ok(sync_op))
        }
      }
    }
  }
}

#[derive(Debug, PartialEq, Eq)]
pub struct BsdiffOpIter<'a, R> {
  reader: &'a mut R,
}

impl<R> Iterator for BsdiffOpIter<'_, R>
where
  R: BufRead,
{
  type Item = Result<bsdiff::Control, String>;

  fn next(&mut self) -> Option<Self::Item> {
    match decode_protobuf::<bsdiff::Control>(&mut self.reader) {
      Err(e) => Some(Err(format!(
        "Couldn't decode Bsdiff Control message from reader!\n{e}"
      ))),

      Ok(control_op) => {
        if control_op.eof {
          // Wharf adds a Rsync HeyYouDidIt message after the Bsdiff EOF
          match decode_protobuf::<pwr::SyncOp>(&mut self.reader) {
            Err(e) => Some(Err(format!(
              "Couldn't decode Rsync SyncOp message from reader!\n{e}"
            ))),

            Ok(sync_op) => {
              if sync_op.r#type() == pwr::sync_op::Type::HeyYouDidIt {
                None
              } else {
                Some(Err(
                  "Expected a Rsync HeyYouDidIt sync operation, but did not found it!".to_string(),
                ))
              }
            }
          }
        } else {
          Some(Ok(control_op))
        }
      }
    }
  }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SyncHeader<'a, R> {
  Rsync {
    file_index: i64,
    op_iter: RsyncOpIter<'a, R>,
  },
  Bsdiff {
    file_index: i64,
    target_index: i64,
    op_iter: BsdiffOpIter<'a, R>,
  },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncEntryIter<R> {
  reader: R,
}

impl<'a, R> SyncEntryIter<R>
where
  R: BufRead,
{
  pub fn next_header(&'a mut self) -> Option<Result<SyncHeader<'a, R>, String>> {
    match self.reader.fill_buf() {
      // If it couldn't read from the stream, return an error
      Err(e) => Some(Err(format!("Couldn't read from reader into buffer!\n{e}"))),

      // If there isn't any data remaining, return None
      Ok([]) => None,

      // If there is data remaining, return the decoded header
      Ok(_) => {
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

        // Pack the gathered data into a SyncHeader struct and return it
        Some(Ok(match bsdiff_header {
          None => SyncHeader::Rsync {
            file_index: header.file_index,
            op_iter: RsyncOpIter {
              reader: &mut self.reader,
            },
          },
          Some(bsdiff) => SyncHeader::Bsdiff {
            file_index: header.file_index,
            target_index: bsdiff.target_index,
            op_iter: BsdiffOpIter {
              reader: &mut self.reader,
            },
          },
        }))
      }
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

fn set_permissions(path: &std::path::Path, mode: u32) -> Result<(), String> {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)
      .map_err(|e| {
        format!(
          "Couldn't read path metadata: \"{}\"\n{e}",
          path.to_string_lossy()
        )
      })?
      .permissions();

    if permissions.mode() != mode {
      permissions.set_mode(mode);

      std::fs::set_permissions(path, permissions).map_err(|e| {
        format!(
          "Couldn't change path permissions: \"{}\"\n{e}",
          path.to_string_lossy()
        )
      })?;
    }
  }

  Ok(())
}

fn apply_container_permissions(
  container: &tlc::Container,
  build_folder: &std::path::Path,
) -> Result<(), String> {
  for file in &container.files {
    set_permissions(&build_folder.join(&file.path), file.mode)?;
  }

  for dir in &container.dirs {
    set_permissions(&build_folder.join(&dir.path), dir.mode)?;
  }

  for sym in &container.symlinks {
    set_permissions(&build_folder.join(&sym.path), sym.mode)?;
  }

  Ok(())
}

fn create_container_symlinks(
  container: &tlc::Container,
  build_folder: &std::path::Path,
) -> Result<(), String> {
  #[cfg(unix)]
  {
    for sym in &container.symlinks {
      let path = build_folder.join(&sym.path);
      let original = build_folder.join(&sym.dest);

      let exists_path = std::fs::exists(&path).map_err(|e| {
        format!(
          "Couldn't check is the path exists: \"{}\"\n{e}",
          path.to_string_lossy()
        )
      })?;

      if !exists_path {
        std::os::unix::fs::symlink(&original, &path).map_err(|e| {
          format!(
            "Couldn't create symlink\n  Original: {}\n  Link: {}\n{e}",
            original.to_string_lossy(),
            path.to_string_lossy()
          )
        })?;
      }
    }
  }

  Ok(())
}

pub fn verify_files(
  build_folder: &std::path::Path,
  signature: &mut Signature<impl BufRead>,
) -> Result<(), String> {
  // This buffer will hold the current block that is being hashed
  let mut buffer = vec![0u8; BLOCK_SIZE];

  // Create a MD5 hasher
  let mut hasher = Md5::new();

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

      // Read the current block
      let buf = &mut buffer[..block_end - block_start];
      file_bufreader.read_exact(buf).map_err(|e| {
        format!(
          "Couldn't read file data into buffer: \"{}\"\n{e}",
          file_path.to_string_lossy()
        )
      })?;

      // Hash the current block
      hasher.update(buf);
      let hash = hasher.finalize_reset();

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

  // Create the symlinks
  create_container_symlinks(&signature.container_new, build_folder)?;

  // Set the correct permissions for the files, folders and symlinks
  apply_container_permissions(&signature.container_new, build_folder)?;

  Ok(())
}

/// <https://docs.itch.zone/wharf/master/file-formats/patches.html>
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
///     - [`pwr::SyncOp`] (Type = `HEY_YOU_DID_IT`)  // end of file’s series
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
  };

  Ok(Patch {
    header,
    container_old,
    container_new,
    sync_op_iter,
  })
}

fn copy_range(
  src: &mut std::fs::File,
  dst: &mut std::fs::File,
  block_index: u64,
  block_span: u64,
) -> Result<(), String> {
  let start_pos = block_index * BLOCK_SIZE as u64;
  let len = block_span * BLOCK_SIZE as u64;

  src
    .seek(std::io::SeekFrom::Start(start_pos))
    .map_err(|e| format!("Couldn't seek into old file at pos: {}\n{e}", start_pos))?;

  let mut limited = src.take(len);

  std::io::copy(&mut limited, dst)
    .map(|_| ())
    .map_err(|e| format!("Couldn't copy data from old file to new!\n {e}"))
}

fn add_bytes(src: &mut std::fs::File, dst: &mut std::fs::File, add: &[u8]) -> Result<(), String> {
  let mut buffer = vec![0u8; add.len()];
  src
    .read_exact(&mut buffer)
    .map_err(|e| format!("Couldn't read data from old file into buffer!\n {e}"))?;

  for (b, a) in buffer.iter_mut().zip(add) {
    *b = b.wrapping_add(*a);
  }

  dst
    .write_all(&buffer)
    .map_err(|e| format!("Couldn't save buffer data into new file!\n {e}"))
}

fn get_container_file(container: &tlc::Container, file_index: usize) -> Result<&tlc::File, String> {
  container
    .files
    .get(file_index)
    .ok_or_else(|| format!("Invalid old file index in patch file!\nIndex: {file_index}"))
}

fn get_old_container_file(
  container: &tlc::Container,
  file_index: usize,
  build_folder: &std::path::Path,
) -> Result<std::fs::File, String> {
  let file_path = build_folder.join(&get_container_file(container, file_index)?.path);

  std::fs::File::open(&file_path).map_err(|e| {
    format!(
      "Couldn't open old file for reading: \"{}\"\n{e}",
      file_path.to_string_lossy()
    )
  })
}

fn get_new_container_file(
  container: &tlc::Container,
  file_index: usize,
  build_folder: &std::path::Path,
) -> Result<std::fs::File, String> {
  let file_path = build_folder.join(&get_container_file(container, file_index)?.path);

  std::fs::OpenOptions::new()
    .create(true)
    .write(true)
    .truncate(true)
    .open(&file_path)
    .map_err(|e| {
      format!(
        "Couldn't open new file for writting: \"{}\"\n{e}",
        file_path.to_string_lossy()
      )
    })
}

pub fn apply_patch(
  old_build_folder: &std::path::Path,
  new_build_folder: &std::path::Path,
  patch: &mut Patch<impl BufRead>,
) -> Result<(), String> {
  // Iterate over the folders in the new container and create them
  for folder in &patch.container_new.dirs {
    let new_folder = new_build_folder.join(&folder.path);
    std::fs::create_dir_all(&new_folder).map_err(|e| {
      format!(
        "Couldn't create folder: \"{}\"\n{e}",
        new_folder.to_string_lossy()
      )
    })?;
  }

  // Create a cache of open file descriptors for the old files
  // The key is the file_index of the old file provided by the patch
  // The value is the open file descriptor
  let mut old_files_cache: lru::LruCache<usize, std::fs::File> =
    lru::LruCache::new(MAX_OPEN_FILES_PATCH);

  // Patch all files in the iterator one by one
  while let Some(header) = patch.sync_op_iter.next_header() {
    let header = header.map_err(|e| format!("Couldn't get next patch sync operation!\n{e}"))?;

    match header {
      // The current file will be updated using the Rsync method
      SyncHeader::Rsync {
        file_index,
        mut op_iter,
      } => {
        // Open the new file
        let mut new_file =
          get_new_container_file(&patch.container_new, file_index as usize, new_build_folder)?;

        // Now apply all the sync operations
        for op in op_iter.by_ref() {
          let op: pwr::SyncOp = op?;

          match op.r#type() {
            // If the type is BlockRange, just copy the range from the old file to the new one
            pwr::sync_op::Type::BlockRange => {
              // Open the old file
              let old_file =
                old_files_cache.try_get_or_insert_mut(op.file_index as usize, || {
                  get_old_container_file(
                    &patch.container_old,
                    op.file_index as usize,
                    old_build_folder,
                  )
                })?;

              // Rewind isn't needed because the copy_range function already seeks
              // into the correct (not relative) position

              // Copy the specified range to the new file
              copy_range(
                old_file,
                &mut new_file,
                op.block_index as u64,
                op.block_span as u64,
              )?;
            }
            // If the type is Data, just copy the data from the patch to the new file
            pwr::sync_op::Type::Data => {
              new_file
                .write_all(&op.data)
                .map_err(|e| format!("Couldn't copy data from patch to new file!\n {e}"))?;
            }
            // If the type is HeyYouDidIt, then the iterator would have returned None
            pwr::sync_op::Type::HeyYouDidIt => unreachable!(),
          }
        }
      }

      // The current file will be updated using the Bsdiff method
      SyncHeader::Bsdiff {
        file_index,
        target_index,
        mut op_iter,
      } => {
        // Open the new file
        let mut new_file =
          get_new_container_file(&patch.container_new, file_index as usize, new_build_folder)?;

        // Open the old file
        let old_file = old_files_cache.try_get_or_insert_mut(target_index as usize, || {
          get_old_container_file(
            &patch.container_old,
            target_index as usize,
            old_build_folder,
          )
        })?;

        // Rewind the old file to the start because the file might
        // have been in the cache and seeked before
        old_file
          .rewind()
          .map_err(|e| format!("Couldn't seek old file to start: {e}"))?;

        // Now apply all the control operations
        for control in op_iter.by_ref() {
          let control = control?;

          // Control operations must be applied in order
          // First, add the diff bytes
          if !control.add.is_empty() {
            add_bytes(old_file, &mut new_file, &control.add)?;
          }

          // Then, copy the extra bytes
          if !control.copy.is_empty() {
            new_file
              .write_all(&control.copy)
              .map_err(|e| format!("Couldn't copy data from patch to new file!\n {e}"))?;
          }

          // Lastly, seek into the correct position in the old file
          if control.seek != 0 {
            old_file.seek_relative(control.seek).map_err(|e| {
              format!(
                "Couldn't seek into old file at relative pos: {}\n{e}",
                control.seek
              )
            })?;
          }
        }
      }
    }
  }

  // Create the symlinks
  create_container_symlinks(&patch.container_new, new_build_folder)?;

  // Set the correct permissions for the files, folders and symlinks
  apply_container_permissions(&patch.container_new, new_build_folder)?;

  Ok(())
}
