use crate::protos::{pwr::CompressionAlgorithm, tlc};

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L33>
pub const BLOCK_SIZE: u64 = 64 * 1024;

/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L30>
const MODE_MASK: u32 = 0o644;

/// Get the number of blocks that a file of a given size occupies
/// 
/// If the file is empty, still count one block for its empty hash
#[inline]
#[must_use]
pub fn file_blocks(size: u64) -> u64 {
  size.div_ceil(BLOCK_SIZE).max(1)
}

/// Verify that the next four bytes of the reader match the expected magic number
///
/// # Errors
///
/// If the bytes couldn't be read from the reader or the magic bytes don't match
pub fn check_magic_bytes(reader: &mut impl Read, expected_magic: u32) -> Result<(), String> {
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

fn set_permissions(path: &Path, mode: u32) -> Result<(), String> {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;

    // Apply the mode mask to set at least the mask permissions
    let mode = mode | MODE_MASK;

    let mut permissions = fs::metadata(path)
      .map_err(|e| {
        format!(
          "Couldn't read path metadata: \"{}\"\n{e}",
          path.to_string_lossy()
        )
      })?
      .permissions();

    if permissions.mode() != mode {
      permissions.set_mode(mode);

      fs::set_permissions(path, permissions).map_err(|e| {
        format!(
          "Couldn't change path permissions: \"{}\"\n{e}",
          path.to_string_lossy()
        )
      })?;
    }
  }

  Ok(())
}

pub fn apply_container_permissions(
  container: &tlc::Container,
  build_folder: &Path,
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

pub fn create_container_symlinks(
  container: &tlc::Container,
  build_folder: &Path,
) -> Result<(), String> {
  #[cfg(unix)]
  {
    for sym in &container.symlinks {
      let path = build_folder.join(&sym.path);
      let original = build_folder.join(&sym.dest);

      let exists_path = fs::exists(&path).map_err(|e| {
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

fn get_container_file(container: &tlc::Container, file_index: usize) -> Result<&tlc::File, String> {
  container
    .files
    .get(file_index)
    .ok_or_else(|| format!("Invalid old file index in patch file!\nIndex: {file_index}"))
}

pub fn get_container_file_read(
  container: &tlc::Container,
  file_index: usize,
  build_folder: &Path,
) -> Result<fs::File, String> {
  let file_path = build_folder.join(&get_container_file(container, file_index)?.path);

  fs::File::open(&file_path).map_err(|e| {
    format!(
      "Couldn't open file for reading: \"{}\"\n{e}",
      file_path.to_string_lossy()
    )
  })
}

pub fn get_container_file_write(
  container: &tlc::Container,
  file_index: usize,
  build_folder: &Path,
) -> Result<fs::File, String> {
  let file_path = build_folder.join(&get_container_file(container, file_index)?.path);

  fs::OpenOptions::new()
    .create(true)
    .write(true)
    .truncate(true)
    .open(&file_path)
    .map_err(|e| {
      format!(
        "Couldn't open file for writting: \"{}\"\n{e}",
        file_path.to_string_lossy()
      )
    })
}
