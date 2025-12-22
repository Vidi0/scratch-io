use crate::errors::FilesystemError;
use crate::{filesystem, game_files};
use std::fs::File;
use std::path::Path;

enum ArchiveFormat {
  Zip,
  Tar,
  TarGz,
  TarBz2,
  TarXz,
  TarZst,
  Other,
}

/// Gets the archive format of the file
///
/// If the file is not an archive, then the format is `ArchiveFormat::Other`
fn get_archive_format(file: &Path) -> Result<ArchiveFormat, FilesystemError> {
  let Ok(extension) = filesystem::get_file_extension(file).map(str::to_lowercase) else {
    return Ok(ArchiveFormat::Other);
  };

  // At this point, we know the file has an extension
  let is_tar_compressed: bool = filesystem::get_file_stem(file)?
    .to_lowercase()
    .ends_with(".tar");

  Ok(match &*extension {
    "zip" => ArchiveFormat::Zip,

    "tar" => ArchiveFormat::Tar,

    "gz" if is_tar_compressed => ArchiveFormat::TarGz,
    "tgz" | "taz" => ArchiveFormat::TarGz,

    "bz2" if is_tar_compressed => ArchiveFormat::TarBz2,
    "tbz" | "tbz2" | "tz2" => ArchiveFormat::TarBz2,

    "xz" if is_tar_compressed => ArchiveFormat::TarXz,
    "txz" => ArchiveFormat::TarXz,

    "zst" if is_tar_compressed => ArchiveFormat::TarZst,
    "tzst" => ArchiveFormat::TarZst,

    _ => ArchiveFormat::Other,
  })
}

/// Extracts the archive into the given folder
///
/// If the file isn't an archive it will be moved to the folder
pub fn extract(file_path: &Path, extract_folder: &Path) -> Result<(), String> {
  // If the extract folder isn't empty, return an error
  filesystem::ensure_is_empty(extract_folder)?;

  let format: ArchiveFormat = get_archive_format(file_path)?;

  // If the file isn't an archive, return now
  if let ArchiveFormat::Other = format {
    // Create the destination folder
    filesystem::create_dir(extract_folder)?;

    // Get the file destination
    let destination = extract_folder.join(filesystem::get_file_name(file_path)?);

    // Move the file
    filesystem::rename(file_path, &destination)?;

    // Make it executable
    filesystem::make_executable(&destination)?;

    return Ok(());
  }

  // The archive will be extracted to the extract_folder_temp, and then moved to its final destination once the extraction is completed
  let extract_folder_temp = game_files::add_part_extension(extract_folder)?;

  // The extraction temporal folder could have contents if a previous extraction was cancelled
  // For that reason, don't check if the folder is empty; but create it if it doesn't exist
  filesystem::create_dir(&extract_folder_temp)?;

  // Open the file in read-only mode
  let file = filesystem::open_file(file_path, std::fs::OpenOptions::new().read(true))?;

  // Extract the archive based on its format
  match format {
    ArchiveFormat::Other => unreachable!("If the format is Other, we should've exited before!"),
    ArchiveFormat::Zip => extract_zip(&file, &extract_folder_temp)?,
    ArchiveFormat::Tar => extract_tar(&file, &extract_folder_temp)?,
    ArchiveFormat::TarGz => extract_tar_gz(&file, &extract_folder_temp)?,
    ArchiveFormat::TarBz2 => extract_tar_bz2(&file, &extract_folder_temp)?,
    ArchiveFormat::TarXz => extract_tar_xz(&file, &extract_folder_temp)?,
    ArchiveFormat::TarZst => extract_tar_zst(&file, &extract_folder_temp)?,
  }

  // Remove the archive
  filesystem::remove_file(file_path)?;

  // If the extraction folder has any common roots, remove them
  game_files::remove_root_folder(&extract_folder_temp)?;

  // Move the temporal folder to its destination
  game_files::move_folder(&extract_folder_temp, extract_folder)?;

  Ok(())
}

#[cfg_attr(not(feature = "zip"), allow(unused_variables))]
fn extract_zip(file: &File, folder: &Path) -> Result<(), String> {
  #[cfg(feature = "zip")]
  {
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    archive
      .extract(folder)
      .map_err(|e| format!("Error extracting ZIP archive: {e}"))
  }

  #[cfg(not(feature = "zip"))]
  {
    Err(
      "This binary was built without ZIP support. Recompile with `--features zip` to be able to extract this archive".to_string()
    )
  }
}

#[cfg_attr(not(feature = "tar"), allow(unused_variables))]
fn extract_tar(file: &File, folder: &Path) -> Result<(), String> {
  #[cfg(feature = "tar")]
  {
    let mut tar_decoder = tar::Archive::new(file);
    tar_decoder
      .unpack(folder)
      .map_err(|e| format!("Error extracting tar archive: {e}"))
  }

  #[cfg(not(feature = "tar"))]
  {
    Err(
      "This binary was built without TAR support. Recompile with `--features tar` to be able to extract this archive".to_string()
    )
  }
}

#[cfg_attr(not(all(feature = "gzip", feature = "tar")), allow(unused_variables))]
fn extract_tar_gz(file: &File, folder: &Path) -> Result<(), String> {
  #[cfg(all(feature = "gzip", feature = "tar"))]
  {
    let gz_decoder = flate2::read::GzDecoder::new(file);
    let mut tar_decoder = tar::Archive::new(gz_decoder);
    tar_decoder
      .unpack(folder)
      .map_err(|e| format!("Error extracting tar.gz archive: {e}"))
  }

  #[cfg(not(all(feature = "gzip", feature = "tar")))]
  {
    Err(
      r#"This binary was built without gzip or TAR support. Recompile with `--features "gzip tar"` to be able to extract this archive"#.to_string()
    )
  }
}

#[cfg_attr(not(all(feature = "bzip2", feature = "tar")), allow(unused_variables))]
fn extract_tar_bz2(file: &File, folder: &Path) -> Result<(), String> {
  #[cfg(all(feature = "bzip2", feature = "tar"))]
  {
    let bz2_decoder = bzip2::read::BzDecoder::new(file);
    let mut tar_decoder = tar::Archive::new(bz2_decoder);
    tar_decoder
      .unpack(folder)
      .map_err(|e| format!("Error extracting tar.gz archive: {e}"))
  }

  #[cfg(not(all(feature = "bzip2", feature = "tar")))]
  {
    Err(
      r#"This binary was built without bzip2 or TAR support. Recompile with `--features "bzip2 tar"` to be able to extract this archive"#.to_string()
    )
  }
}

#[cfg_attr(not(all(feature = "xz", feature = "tar")), allow(unused_variables))]
fn extract_tar_xz(file: &File, folder: &Path) -> Result<(), String> {
  #[cfg(all(feature = "xz", feature = "tar"))]
  {
    let xz_decoder = liblzma::read::XzDecoder::new(file);
    let mut tar_decoder = tar::Archive::new(xz_decoder);
    tar_decoder
      .unpack(folder)
      .map_err(|e| format!("Error extracting tar.xz archive: {e}"))
  }

  #[cfg(not(all(feature = "xz", feature = "tar")))]
  {
    Err(
      r#"This binary was built without XZ or TAR support. Recompile with `--features "xz tar"` to be able to extract this archive"#.to_string()
    )
  }
}

#[cfg_attr(not(all(feature = "zstd", feature = "tar")), allow(unused_variables))]
fn extract_tar_zst(file: &File, folder: &Path) -> Result<(), String> {
  #[cfg(all(feature = "zstd", feature = "tar"))]
  {
    let zstd_decoder =
      zstd::Decoder::new(file).map_err(|e| format!("Error reading tar.zst archive: {e}"))?;
    let mut tar_decoder = tar::Archive::new(zstd_decoder);
    tar_decoder
      .unpack(folder)
      .map_err(|e| format!("Error extracting tar.zst archive: {e}"))
  }

  #[cfg(not(all(feature = "zstd", feature = "tar")))]
  {
    Err(
      r#"This binary was built without Zstd or TAR support. Recompile with `--features "zstd tar"` to be able to extract this archive"#.to_string()
    )
  }
}
