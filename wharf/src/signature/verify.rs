use super::read::Signature;
use crate::common::{BLOCK_SIZE, apply_container_permissions, create_container_symlinks};
use md5::{Digest, Md5};

use std::io::Read;
use std::path::Path;

impl Signature<'_> {
  pub fn verify(
    &mut self,
    build_folder: &Path,
    mut progress_callback: impl FnMut(u64),
  ) -> Result<Vec<usize>, String> {
    // This vector holds the indexes of the broken files
    let mut broken_files: Vec<usize> = Vec::new();

    // This buffer will hold the current block that is being hashed
    let mut buffer = vec![0u8; BLOCK_SIZE as usize];

    // Create a MD5 hasher
    let mut hasher = Md5::new();

    // Loop over all the files in the signature container
    'file: for (file_index, container_file) in self.container_new.files.iter().enumerate() {
      // Get file path
      let file_path = container_file.get_path(build_folder.to_owned())?;

      let file_size = container_file.size as u64;

      // Check if the file exists
      let exists = file_path
        .try_exists()
        .map_err(|e| format!("Couldn't check if the file exists!\n{e}"))?;

      if !exists {
        broken_files.push(file_index);
        let skipped_blocks = self.block_hash_iter.skip_file(file_size, 0)?;
        progress_callback(skipped_blocks);
        continue 'file;
      }

      // Check if the file length matches
      let metadata = file_path
        .metadata()
        .map_err(|e| format!("Couldn't get file metadata!\n{e}"))?;

      if metadata.len() != file_size {
        broken_files.push(file_index);
        let skipped_blocks = self.block_hash_iter.skip_file(file_size, 0)?;
        progress_callback(skipped_blocks);
        continue 'file;
      }

      // Wrapping the file inside a BufReader isn't needed because
      // BLOCK_SIZE is already large
      let mut file = container_file.open_read(&file_path)?;

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
          .map_err(|e| format!("Couldn't read file data into buffer!\n{e}"))?;

        // Hash the current block
        hasher.update(buf);
        let hash = hasher.finalize_reset();

        // Get the expected hash from the signature
        let signature_hash = self.block_hash_iter.next().ok_or_else(|| {
          "Expected a block hash message in the signature, but EOF was encountered!".to_string()
        })??;

        // One new hash has been read, callback!
        progress_callback(1);

        // Compare the hashes
        if *signature_hash.strong_hash != *hash {
          broken_files.push(file_index);
          let skipped_blocks = self.block_hash_iter.skip_file(file_size, block_index + 1)?;
          progress_callback(skipped_blocks);
          continue 'file;
        }

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

    Ok(broken_files)
  }
}
