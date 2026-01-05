use crate::protos::{pwr::CompressionAlgorithm, tlc};

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L33>
pub const BLOCK_SIZE: u64 = 64 * 1024;

/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L30>
const MIN_MODE: u32 = 0o644;
const MAX_MODE: u32 = 0o777;

/// Clamp the given mode between the minimum and maximum
///
/// Clamping the mode ensures that it is valid
#[inline]
#[must_use]
pub fn mask_mode(mode: u32) -> u32 {
  (mode & MAX_MODE) | MIN_MODE
}

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

    let exists = fs::exists(path).map_err(|e| {
      format!(
        "Couldn't check if the path exists: \"{}\"\n{e}",
        path.to_string_lossy()
      )
    })?;

    if !exists {
      return Ok(());
    }

    // Apply the mode mask to set at least the mask permissions
    let mode = mask_mode(mode);

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

fn path_safe_push(base: &mut PathBuf, extension: &Path) -> Result<(), String> {
  for comp in extension.components() {
    match comp {
      std::path::Component::Normal(p) => base.push(p),
      std::path::Component::CurDir => (),

      // Any other component is not safe!
      _ => return Err(format!("The extension is not safe! It contains: {comp:?}")),
    }
  }

  Ok(())
}

pub trait ContainerItem {
  fn mode(&self) -> u32;
  fn path(&self) -> &str;

  fn get_path(&self, mut build_folder: PathBuf) -> Result<PathBuf, String> {
    path_safe_push(&mut build_folder, Path::new(self.path()))?;
    Ok(build_folder)
  }
}

impl ContainerItem for tlc::Dir {
  fn mode(&self) -> u32 {
    self.mode
  }

  fn path(&self) -> &str {
    &self.path
  }
}

impl ContainerItem for tlc::File {
  fn mode(&self) -> u32 {
    self.mode
  }

  fn path(&self) -> &str {
    &self.path
  }
}

impl tlc::File {
  pub fn open_read(&self, file_path: &Path) -> Result<fs::File, String> {
    fs::File::open(file_path).map_err(|e| {
      format!(
        "Couldn't open file for reading: \"{}\"\n{e}",
        file_path.to_string_lossy()
      )
    })
  }

  pub fn open_write(&self, file_path: &Path) -> Result<fs::File, String> {
    fs::OpenOptions::new()
      .create(true)
      .write(true)
      .truncate(true)
      .open(file_path)
      .map_err(|e| {
        format!(
          "Couldn't open file for writting: \"{}\"\n{e}",
          file_path.to_string_lossy()
        )
      })
  }
}

impl tlc::Container {
  pub fn get_file(&self, index: usize) -> Result<&tlc::File, String> {
    self
      .files
      .get(index)
      .ok_or_else(|| format!("Invalid old file index: {index}!"))
  }

  pub fn get_file_path(&self, index: usize, build_folder: PathBuf) -> Result<PathBuf, String> {
    self.get_file(index)?.get_path(build_folder)
  }

  pub fn open_file_read(&self, index: usize, build_folder: PathBuf) -> Result<fs::File, String> {
    let file = self.get_file(index)?;
    file.open_read(&file.get_path(build_folder)?)
  }

  pub fn open_file_write(&self, index: usize, build_folder: PathBuf) -> Result<fs::File, String> {
    let file = self.get_file(index)?;
    file.open_write(&file.get_path(build_folder)?)
  }

  pub fn create_directories(&self, build_folder: &Path) -> Result<(), String> {
    // Iterate over the folders in the container and create them
    for dir in &self.dirs {
      let dir_path = dir.get_path(build_folder.to_owned())?;

      // This function call will do nothing if the directory already exists
      fs::create_dir_all(&dir_path).map_err(|e| {
        format!(
          "Couldn't create directory: \"{}\"\n{e}",
          dir_path.to_string_lossy()
        )
      })?;

      // Change the permissions
      set_permissions(&dir_path, dir.mode)?;
    }

    Ok(())
  }
}
