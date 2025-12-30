use super::read::Signature;
use crate::common::{BLOCK_SIZE, apply_container_permissions, create_container_symlinks};
use md5::{Digest, Md5};

use std::fs;
use std::io::Read;
use std::path::Path;

pub fn verify_files(
  build_folder: &Path,
  signature: &mut Signature,
  mut progress_callback: impl FnMut(),
) -> Result<(), String> {
  // This buffer will hold the current block that is being hashed
  let mut buffer = vec![0u8; BLOCK_SIZE as usize];

  // Create a MD5 hasher
  let mut hasher = Md5::new();

  // Loop over all the files in the signature container
  for container_file in &signature.container_new.files {
    let file_path = build_folder.join(&container_file.path);
    // Wrapping the file inside a BufReader isn't needed because
    // BLOCK_SIZE is already large
    let mut file = fs::File::open(&file_path).map_err(|e| {
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
    let mut block_index: u64 = 0;

    loop {
      // The size of the current block is BLOCK_SIZE,
      // unless there are less remaining bytes on the file
      let current_block_size =
        BLOCK_SIZE.min(container_file.size as u64 - block_index * BLOCK_SIZE);

      // Read the current block
      let buf = &mut buffer[..current_block_size as usize];
      file.read_exact(buf).map_err(|e| {
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

      // One new hash has been verified, callback!
      progress_callback();

      // If the file has been fully read, proceed to the next one
      if block_index * BLOCK_SIZE + current_block_size == container_file.size as u64 {
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
