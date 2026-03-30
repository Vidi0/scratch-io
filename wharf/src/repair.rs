use super::Signature;
use super::verify::IntegrityIssues;
use crate::pool::{ContainerPool, Pool, PoolError, WritablePool, ZipPool};

use rc_zip_sync::{ArchiveHandle, HasCursor};
use std::path::Path;

/// Repair broken entries in a pool using another pool as the source
///
/// For each broken entry listed in `integrity_issues`, the entry is copied
/// from `src_pool` into `dst_pool`, overwriting the broken entry.
/// Progress is reported through `progress_callback` with the number of
/// bytes written since the last call.
///
/// This function will NOT create missing folders, symlinks, or check if
/// the modes (permissions) of the files, folders, and symlinks are correct.
/// It will fail if a file's parent folder does not exist.
///
/// # Arguments
///
/// * `integrity_issues` - A struct containing the indexes of the broken entries
///
/// * `dst_pool` - The pool containing the broken entries to be repaired
///
/// * `src_pool` - The pool to read the correct entries from
///
/// * `progress_callback` - A callback invoked with the number of bytes
///   written since the last call
///
/// # Errors
///
/// If an entry is missing in `src_pool` or there is an I/O failure while
/// reading or writing.
pub fn repair_files(
  integrity_issues: &IntegrityIssues,
  dst_pool: &mut impl WritablePool,
  src_pool: &mut impl Pool,
  mut progress_callback: impl FnMut(u64) + Send,
) -> Result<(), PoolError> {
  for &entry_index in &integrity_issues.files {
    let bytes = dst_pool.copy_from(entry_index, src_pool)?;
    progress_callback(bytes);
  }

  Ok(())
}

impl Signature<'_> {
  /// Prepare the build folder and repair the broken files
  ///
  /// This function will:
  /// 1. Create all directories, files, and symlinks described in
  ///    [`Self::container_new`] and set their modes (permissions)
  /// 2. Repair the broken files
  ///
  /// After this function is called, the build folder will contain all the
  /// files, directories and symlinks described in the container with the
  /// correct permissions, and all files will have the correct contents.
  ///
  ///  # Arguments
  ///
  /// * `build_folder` - The path to the build folder
  ///
  /// * `container` - A container describing the filesystem state of the build
  ///   folder
  ///
  /// * `build_zip_archive` - A reference to a ZIP archive handle containing the
  ///   source files. Each file in `integrity_issues.files` must exist in the
  ///   archive
  ///
  /// * `progress_callback` - A callback that is called with the number of
  ///   bytes written since the last one
  ///
  /// # Errors
  ///
  /// If a file listed in the container is missing in the ZIP archive or
  /// there is an I/O failure while reading files or metadata.
  pub fn repair<'ar, C>(
    &self,
    integrity_issues: &IntegrityIssues,
    build_folder: &Path,
    build_zip_archive: &'ar ArchiveHandle<C>,
    progress_callback: impl FnMut(u64) + Send,
  ) -> Result<(), PoolError>
  where
    C: HasCursor,
    <C as HasCursor>::Cursor<'ar>: Send,
  {
    // Create the folders, files and symlinks in the destination container
    let mut dst_pool = ContainerPool::create(&self.container_new, build_folder)?;
    let mut src_pool = ZipPool::new(&self.container_new, build_zip_archive);

    repair_files(
      integrity_issues,
      &mut dst_pool,
      &mut src_pool,
      progress_callback,
    )
  }
}
