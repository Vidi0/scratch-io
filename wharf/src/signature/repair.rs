use super::read::Signature;
use super::verify::IntegrityIssues;
use crate::container::ContainerItem;

use rc_zip_sync::{ArchiveHandle, HasCursor};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

impl Signature<'_> {
  /// Repair the files in a build folder using a [ZIP archive][ArchiveHandle]
  /// as the source
  ///
  /// For each missing or broken file listed in `integrity_issues`:
  /// 1. Get the file information from the container in [`Self::container_new`].
  /// 2. Look up the file in the provided `build_zip_archive`.
  /// 3. Read its contents in a streaming, buffered fashion.
  /// 4. Write the contents to the corresponding location in `build_folder`.
  /// 5. Report progress through the `progress_callback` for each chunk
  ///    written, returning the number of bytes written since the last call.
  ///
  /// This function will NOT create missing folders, symlinks, or check if
  /// the modes (permissions) of the files, folders, and symlinks are correct.
  /// It will fail if a file's parent folder does not exist.
  ///
  /// # Arguments
  ///
  /// * `integrity_issues` - A struct containing the indexes of the broken files
  ///
  /// * `build_folder` - The path to the build folder
  ///
  /// * `build_zip_archive` - A reference to a ZIP archive handle containing the
  ///   source files. Each file in `integrity_issues.files` must exist in the archive
  ///
  /// * `progress_callback` - A callback that is called with the number of
  ///   bytes written since the last one
  ///
  /// # Errors
  ///
  /// If a file listed in the container is missing in the ZIP archive or
  /// there is an I/O failure while reading files or metadata.
  ///
  /// # Panics
  ///
  /// If any index in `integrity_issues.filess` is out of bounds for the
  /// container's file list.
  pub fn repair_files(
    &self,
    integrity_issues: &IntegrityIssues,
    build_folder: &Path,
    build_zip_archive: &ArchiveHandle<'_, impl HasCursor>,
    mut progress_callback: impl FnMut(u64),
  ) -> Result<(), String> {
    for &file_index in &integrity_issues.files {
      let container_file = &self.container_new.files[file_index];

      let zip_file = build_zip_archive
        .by_name(&container_file.path)
        .ok_or_else(|| {
          format!(
            "Expected to find the file in the ZIP build archive: \"{}\"",
            &container_file.path
          )
        })?;
      let mut zip_file_reader = BufReader::new(zip_file.reader());

      let file_path = container_file.get_path(build_folder.to_owned())?;
      let mut file = container_file.open_write(&file_path)?;

      loop {
        let buffer = zip_file_reader
          .fill_buf()
          .map_err(|e| format!("Couldn't fill ZIP data buffer!\n{e}"))?;

        if buffer.is_empty() {
          break;
        }

        file
          .write_all(buffer)
          .map_err(|e| format!("Couldn't write ZIP data into file!\n{e}"))?;

        let len = buffer.len();
        progress_callback(len as u64);
        zip_file_reader.consume(len);
      }
    }

    Ok(())
  }
}
