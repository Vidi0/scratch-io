use super::read::Signature;
use crate::common::{
  BLOCK_SIZE, apply_container_permissions, create_container_symlinks, get_container_file_read,
};
use md5::{Digest, Md5};

use std::io::Read;
use std::path::Path;

impl Signature<'_> {
  pub fn verify(
    &mut self,
    build_folder: &Path,
    mut progress_callback: impl FnMut(),
  ) -> Result<(), String> {
    // This buffer will hold the current block that is being hashed
    let mut buffer = vec![0u8; BLOCK_SIZE as usize];

    // Create a MD5 hasher
    let mut hasher = Md5::new();

    // Loop over all the files in the signature container
    'file: for file_index in 0..self.container_new.files.len() {
      // Wrapping the file inside a BufReader isn't needed because
      // BLOCK_SIZE is already large
      let mut file = get_container_file_read(&self.container_new, file_index, build_folder)?;

      // Check if the file length matches
      let file_size = self.container_new.files[file_index].size as u64;

      let metadata = file
        .metadata()
        .map_err(|e| format!("Couldn't get file metadata!\n{e}"))?;

      if metadata.len() != file_size {
        return Err(format!(
          "The signature and the in-disk size of the file with index \"{file_index}\" don't match!",
        ));
      }

      // For each block in the file, compare its hash with the one provided in the signature
      let mut block_index: u64 = 0;

      loop {
        // The size of the current block is BLOCK_SIZE,
        // unless there are less remaining bytes on the file
        let current_block_size = BLOCK_SIZE.min(file_size - block_index * BLOCK_SIZE);

        // Read the current block
        let buf = &mut buffer[..current_block_size as usize];
        file
          .read_exact(buf)
          .map_err(|e| format!("Couldn't read file data into buffer!\n{e}",))?;

        // Hash the current block
        hasher.update(buf);
        let hash = hasher.finalize_reset();

        // Get the expected hash from the signature
        let signature_hash = self.block_hash_iter.next().ok_or_else(|| {
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
        if block_index * BLOCK_SIZE + current_block_size == file_size {
          continue 'file;
        }

        block_index += 1;
      }
    }

    // Create the symlinks
    create_container_symlinks(&self.container_new, build_folder)?;

    // Set the correct permissions for the files, folders and symlinks
    apply_container_permissions(&self.container_new, build_folder)?;

    Ok(())
  }
}
