/// Funcions and structures for reading wharf patches
pub mod patch;
/// Funcions and structures for reading wharf signatures
pub mod signature;

mod common;
mod protos;

use common::BLOCK_SIZE;
use patch::Patch;
use protos::{pwr, tlc};
use signature::Signature;

use md5::{Digest, Md5};
use std::fs;
use std::io::{self, BufRead, Read, Seek, Write};
use std::path::Path;

/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L30>
const MODE_MASK: u32 = 0o644;

const MAX_OPEN_FILES_PATCH: std::num::NonZeroUsize = std::num::NonZeroUsize::new(16).unwrap();

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

fn apply_container_permissions(
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

fn create_container_symlinks(
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

pub fn verify_files(
  build_folder: &Path,
  signature: &mut Signature<impl BufRead>,
) -> Result<(), String> {
  // This buffer will hold the current block that is being hashed
  let mut buffer = vec![0u8; BLOCK_SIZE as usize];

  // Create a MD5 hasher
  let mut hasher = Md5::new();

  // Loop over all the files in the signature container
  for container_file in &signature.container_new.files {
    let file_path = build_folder.join(&container_file.path);
    let file = fs::File::open(&file_path).map_err(|e| {
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
    let mut file_bufreader = io::BufReader::new(file);
    let mut block_index: usize = 0;

    loop {
      let block_start: usize = block_index * BLOCK_SIZE as usize;
      let block_end: usize = std::cmp::min(
        block_start + BLOCK_SIZE as usize,
        container_file.size as usize,
      );

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

fn copy_range(
  src: &mut (impl Read + Seek),
  dst: &mut impl Write,
  block_index: u64,
  block_span: u64,
) -> Result<(), String> {
  let start_pos = block_index * BLOCK_SIZE;
  let len = block_span * BLOCK_SIZE;

  src
    .seek(io::SeekFrom::Start(start_pos))
    .map_err(|e| format!("Couldn't seek into old file at pos: {}\n{e}", start_pos))?;

  let mut limited = src.take(len);

  io::copy(&mut limited, dst)
    .map(|_| ())
    .map_err(|e| format!("Couldn't copy data from old file to new!\n {e}"))
}

fn add_bytes(
  src: &mut impl Read,
  dst: &mut impl Write,
  add: &[u8],
  add_buffer: &mut [u8],
) -> Result<(), String> {
  assert_eq!(add.len(), add_buffer.len());

  src
    .read_exact(add_buffer)
    .map_err(|e| format!("Couldn't read data from old file into buffer!\n {e}"))?;

  for i in 0..add.len() {
    add_buffer[i] += add[i];
  }

  dst
    .write_all(add_buffer)
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
  build_folder: &Path,
) -> Result<fs::File, String> {
  let file_path = build_folder.join(&get_container_file(container, file_index)?.path);

  fs::File::open(&file_path).map_err(|e| {
    format!(
      "Couldn't open old file for reading: \"{}\"\n{e}",
      file_path.to_string_lossy()
    )
  })
}

fn get_new_container_file(
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
        "Couldn't open new file for writting: \"{}\"\n{e}",
        file_path.to_string_lossy()
      )
    })
}

pub fn apply_patch(
  old_build_folder: &Path,
  new_build_folder: &Path,
  patch: &mut Patch<impl BufRead>,
) -> Result<(), String> {
  // Iterate over the folders in the new container and create them
  for folder in &patch.container_new.dirs {
    let new_folder = new_build_folder.join(&folder.path);
    fs::create_dir_all(&new_folder).map_err(|e| {
      format!(
        "Couldn't create folder: \"{}\"\n{e}",
        new_folder.to_string_lossy()
      )
    })?;
  }

  // Create a cache of open file descriptors for the old files
  // The key is the file_index of the old file provided by the patch
  // The value is the open file descriptor
  let mut old_files_cache: lru::LruCache<usize, fs::File> =
    lru::LruCache::new(MAX_OPEN_FILES_PATCH);

  // This buffer is used when applying bsdiff add operations
  // It is created here to avoid allocating and deallocating
  // the buffer on each add operation
  let mut add_buffer: Vec<u8> = Vec::new();

  // Patch all files in the iterator one by one
  while let Some(header) = patch.sync_op_iter.next_header() {
    let header = header.map_err(|e| format!("Couldn't get next patch sync operation!\n{e}"))?;

    match header {
      // The current file will be updated using the Rsync method
      patch::SyncHeader::Rsync {
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
      patch::SyncHeader::Bsdiff {
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
            // Resize the add buffer to match the size of the current add bytes
            // The add operations are usually the same length, so allocation is almost never triggered
            // If the new add bytes are smaller than the buffer size, allocation will also be avoided
            add_buffer.resize(control.add.len(), 0);

            add_bytes(old_file, &mut new_file, &control.add, &mut add_buffer)?;
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
