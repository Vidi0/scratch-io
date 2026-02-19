mod staging;

use super::Patch;
use super::operations::FilesCache;
use crate::hasher::BlockHasher;
use crate::signature::BlockHashIter;

use std::io::Read;
use std::path::Path;

impl Patch<'_> {
  /// Apply the patch operations to produce the new build.
  ///
  /// This creates all files, directories, and symlinks in `new_build_folder`,
  /// then applies each sync operation (rsync or bsdiff) using data from
  /// `old_build_folder`. Written data is hashed on the fly and verified against
  /// `hash_iter` (if provided). `progress_callback` is invoked with the number
  /// of processed bytes as the patch is applied.
  ///
  /// # Arguments
  ///
  /// * `old_build_folder` - The path to the old build folder
  ///
  /// * `new_build_folder` - The path to the new build folder
  ///
  /// * `hash_iter` - Iterator over expected block hashes used to verify the
  ///   integrity of the written files (optional)
  ///
  /// * `progress_callback` - A callback that is called with the number of
  ///   bytes processed since the last one
  ///
  /// # Errors
  ///
  /// If there is an I/O failure while reading files or metadata, or if hash
  /// verification of the generated files fails
  pub fn apply(
    &mut self,
    old_build_folder: &Path,
    new_build_folder: &Path,
    hash_iter: Option<&mut BlockHashIter<impl Read>>,
    mut progress_callback: impl FnMut(u64),
  ) -> Result<(), String> {
    // Create the new container folders, files and symlinks,
    // applying all the correct permissions
    self.container_new.create(new_build_folder)?;

    // Create a cache of open file descriptors for the old files
    // The key is the file_index of the old file provided by the patch
    // The value is the open file descriptor
    let mut old_files_cache = FilesCache::new(old_build_folder);

    // This buffer is used when applying rsync block_range operations and
    // bsdiff add operations. It is created here to avoid allocating and
    // deallocating the buffer on each patch operation.
    //
    // If the buffer was used by the two operations at the same time, it
    // would be very expensive to resize the vector every time the kind of
    // operation changes: block_range operations use a fixed size set by us;
    // while the add operations' length is provided by the patch (every
    // operation should be the same size).
    //
    // However, the buffer can be shared between the two because the patch
    // is either a rsync patch, and only block_range operations are used;
    // or is a bsdiff patch, and then only add operations are used (the patch
    // may contain block_range operations, but for copying whole files
    // unchanged, so a buffer isn't needed anyways).
    let mut patch_op_buffer: Vec<u8> = Vec::new();

    // If a hash_iter was provided, create a reusable hasher
    // instance to verify that the new game files are intact
    let mut hasher = hash_iter.map(|iter| BlockHasher::new(iter));

    // Patch all files in the iterator one by one
    while let Some(header) = self.sync_op_iter.next_header() {
      let mut header =
        header.map_err(|e| format!("Couldn't get next patch sync operation!\n{e}"))?;

      // Open the new file
      let new_container_file = self.container_new.get_file(header.file_index as usize)?;
      let new_file = || new_container_file.open_write(new_build_folder.to_owned());

      // Create a hasher for the current file
      let mut file_hasher = match hasher.as_mut() {
        Some(h) => Some(h.new_file_hasher(new_container_file.block_count())?),
        None => None,
      };

      // Write all the new data into the file
      let status = header.patch_file(
        new_file,
        &mut file_hasher,
        new_container_file.size as u64,
        &mut old_files_cache,
        &self.container_old,
        &mut patch_op_buffer,
        &mut progress_callback,
        /////// TODO: Store checkpoints somewhere
        None,
        &mut |_checkpoint| (),
      )?;

      /////// TODO: DO SOMETHING WITH THE STATUS
      println!("file {}: {:?}", header.file_index, status);
    }

    Ok(())
  }
}
