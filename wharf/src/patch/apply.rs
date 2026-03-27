mod staging;

use super::Patch;
use crate::hasher::BlockHasher;
use crate::pool::{ContainerPool, StagingPool};
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
  /// of written bytes as the patch is applied.
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
  ///   bytes written since the last one
  ///
  /// # Errors
  ///
  /// If there is an I/O failure while reading files or metadata, or if hash
  /// verification of the generated files fails
  pub fn apply(
    &mut self,
    old_build_folder: &Path,
    staging_folder: &Path,
    new_build_folder: &Path,
    hash_iter: Option<&mut BlockHashIter<impl Read>>,
    mut progress_callback: impl FnMut(u64),
  ) -> Result<(), String> {
    // Create the new container folders, files and symlinks,
    // applying all the correct permissions
    let mut dst_pool = ContainerPool::create(&self.container_new, new_build_folder)?;

    // Create the staging folder
    let mut staging_pool = StagingPool::create(staging_folder)?;

    // Create a pool for the old files
    let mut src_pool = ContainerPool::open(&self.container_old, old_build_folder);

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
    let mut hasher = hash_iter.map(|iter| BlockHasher::new(&self.container_new, iter));

    // Reconstruct all the modified files into the staging folder
    let status = staging::reconstruct_modified_files(
      &mut src_pool,
      &mut staging_pool,
      &mut dst_pool,
      &mut self.sync_op_iter,
      &mut hasher,
      &mut patch_op_buffer,
      &mut progress_callback,
    )?;

    ///////// TODO: do something with the status
    for (file_index, file_status) in status.patched_files.into_iter().enumerate() {
      println!("file {}: {:?}", file_index, file_status);
    }

    Ok(())
  }
}
