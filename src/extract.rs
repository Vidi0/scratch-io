use crate::game_files_operations::*;
use crate::game_files_operations::FilesystemError;
use std::path::{Path, PathBuf};
use std::fs::File;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum ArchiveFormat {
  Zip,
  Tar,
  TarGz,
  TarBz2,
  TarXz,
  TarZst,
  Other,
}

#[derive(Error, Debug)]
#[error("Couldn't get the file stem of: \"{path}\"\n{error}")]
pub struct GetArchiveFormatError {
  path: PathBuf,
  #[source]
  error: FilesystemError,
}

/// Gets the archive format of the file
/// 
/// If the file is not an archive, then the format is `ArchiveFormat::Other`
fn get_archive_format(file: &Path) -> Result<ArchiveFormat, GetArchiveFormatError> {
  let Some(extension) = file.extension().map(|e| e.to_string_lossy().to_lowercase()) else {
    return Ok(ArchiveFormat::Other)
  };

  // At this point, we know the file has an extension
  let is_tar_compressed: bool = get_file_stem(file)
    .map_err(|error| GetArchiveFormatError { path: file.to_path_buf(), error })?
    .to_lowercase()
    .ends_with(".tar");

  Ok(
    match extension.as_str() {
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
    }
  )
}

#[derive(Error, Debug)]
pub enum ExtractError {
  #[error("A filesystem error occured:\n{0}")]
  FilesystemError(#[from] FilesystemError),

  #[error("Couldn't get the archive format of the file to extract!\n{0}")]
  CouldntGetArchiveFormat(#[from] GetArchiveFormatError),

  #[error("Couldn't extract the archive:\"{path}\"\n{error}")]
  CouldntExtractArchive {
    path: PathBuf,
    #[source]
    error: ExtractSpecificFormatError,
  },
}

/// Extracts the archive into the given folder
///
/// If the file isn't an archive it will be moved to the folder
pub async fn extract(file_path: &Path, extract_folder: &Path) -> Result<(), ExtractError> {
  // If the extract folder isn't empty, return an error
  if !is_folder_empty(extract_folder)? {
    return Err(FilesystemError::FolderShouldBeEmpty(extract_folder.to_path_buf()).into());
  }
  
  let format: ArchiveFormat = get_archive_format(file_path)?;

  // If the file isn't an archive, return now
  if let ArchiveFormat::Other = format {
    // Create the destination folder
    tokio::fs::create_dir_all(&extract_folder).await
      .map_err(|error| FilesystemError::CreateFolder { error, path: extract_folder.to_path_buf() })?;

    // Get the file destination
    let destination = extract_folder.join(
      file_path.file_name().ok_or_else(|| FilesystemError::PathWithoutFilename(file_path.to_path_buf()))?
    );

    // Move the file
    tokio::fs::rename(file_path, &destination).await
      .map_err(|error| FilesystemError::Move { error, src: file_path.to_path_buf(), dst: destination.to_path_buf() })?;

    // Make it executable
    crate::make_executable(&destination)?;

    return Ok(());
  }

  // The archive will be extracted to the extract_folder_temp, and then moved to its final destination once the extraction is completed
  let extract_folder_temp = add_part_extension(extract_folder)?;

  // The extraction temporal folder could have contents if a previous extraction was cancelled
  // For that reason, don't check if the folder is empty; but create it if it doesn't exist
  tokio::fs::create_dir_all(&extract_folder_temp).await
    .map_err(|error| FilesystemError::CreateFolder { error, path: extract_folder_temp.to_path_buf() })?;

  // Open the file in read-only mode
  let file = File::open(file_path)
    .map_err(|error| FilesystemError::OpenFile { error, path: file_path.to_path_buf() })?;

  // Extract the archive based on its format
  match format {
    ArchiveFormat::Other => unreachable!("If the format is Other, we should've exited before!"),
    ArchiveFormat::Zip => extract_zip(&file, &extract_folder_temp),
    ArchiveFormat::Tar => extract_tar(&file, &extract_folder_temp),
    ArchiveFormat::TarGz => extract_tar_gz(&file, &extract_folder_temp),
    ArchiveFormat::TarBz2 => extract_tar_bz2(&file, &extract_folder_temp),
    ArchiveFormat::TarXz => extract_tar_xz(&file, &extract_folder_temp),
    ArchiveFormat::TarZst => extract_tar_zst(&file, &extract_folder_temp),
  }.map_err(|error| ExtractError::CouldntExtractArchive { path: file_path.to_path_buf(), error })?;

  // Remove the archive
  tokio::fs::remove_file(file_path).await
    .map_err(|error| FilesystemError::RemoveFile { error, path: file_path.to_path_buf() })?;

  // If the extraction folder has any common roots, remove them
  remove_root_folder(&extract_folder_temp).await?;
  
  // Move the temporal folder to its destination
  move_folder(&extract_folder_temp, extract_folder).await?;

  Ok(())
}

#[derive(Error, Debug)]
pub enum ExtractSpecificFormatError {
  #[error("Couldn't extract ZIP archive:\n{0}")]
  Zip(#[from] zip::result::ZipError),

  #[error("Couldn't extract {format:?} archive\n{error}")]
  Other {
    format: ArchiveFormat,
    #[source]
    error: std::io::Error,
  },
}

#[cfg_attr(not(feature = "zip"), allow(unused_variables))]
fn extract_zip(file: &File, folder: &Path) -> Result<(), ExtractSpecificFormatError> {
  #[cfg(feature = "zip")] 
  {
    let mut archive = zip::ZipArchive::new(file)?;
    archive.extract(folder).map_err(|e| e.into())
  }

  #[cfg(not(feature = "zip"))]
  {
    Err(format!("This binary was built without ZIP support. Recompile with `--features zip` to be able to extract this archive"))
  }
}

#[cfg_attr(not(feature = "tar"), allow(unused_variables))]
fn extract_tar(file: &File, folder: &Path) -> Result<(), ExtractSpecificFormatError> {
  #[cfg(feature = "tar")]
  {
    let mut tar_decoder = tar::Archive::new(file);
    tar_decoder.unpack(folder).map_err(|error| ExtractSpecificFormatError::Other { format: ArchiveFormat::Tar, error })
  }

  #[cfg(not(feature = "tar"))]
  {
    Err(format!("This binary was built without TAR support. Recompile with `--features tar` to be able to extract this archive"))
  }
}

#[cfg_attr(not(feature = "gzip"), allow(unused_variables))]
fn extract_tar_gz(file: &File, folder: &Path) -> Result<(), ExtractSpecificFormatError> {
  #[cfg(feature = "gzip")]
  {
    let gz_decoder = flate2::read::GzDecoder::new(file);
    let mut tar_decoder = tar::Archive::new(gz_decoder);
    tar_decoder.unpack(folder).map_err(|error| ExtractSpecificFormatError::Other { format: ArchiveFormat::TarGz, error })
  }

  #[cfg(not(feature = "gzip"))]
  {
    Err(format!("This binary was built without gzip support. Recompile with `--features gzip` to be able to extract this archive"))
  }
}

#[cfg_attr(not(feature = "bzip2"), allow(unused_variables))]
fn extract_tar_bz2(file: &File, folder: &Path) -> Result<(), ExtractSpecificFormatError> {
  #[cfg(feature = "bzip2")]
  {
    let bz2_decoder = bzip2::read::BzDecoder::new(file);
    let mut tar_decoder = tar::Archive::new(bz2_decoder);
    tar_decoder.unpack(folder).map_err(|error| ExtractSpecificFormatError::Other { format: ArchiveFormat::TarBz2, error })
  }

  #[cfg(not(feature = "bzip2"))]
  {
    Err(format!("This binary was built without bzip2 support. Recompile with `--features bzip2` to be able to extract this archive"))
  }
}

#[cfg_attr(not(feature = "xz"), allow(unused_variables))]
fn extract_tar_xz(file: &File, folder: &Path) -> Result<(), ExtractSpecificFormatError> {
  #[cfg(feature = "xz")]
  {
    let xz_decoder = liblzma::read::XzDecoder::new(file);
    let mut tar_decoder = tar::Archive::new(xz_decoder);
    tar_decoder.unpack(folder).map_err(|error| ExtractSpecificFormatError::Other { format: ArchiveFormat::TarXz, error })
  }
  
  #[cfg(not(feature = "xz"))]
  {
    Err(format!("This binary was built without XZ support. Recompile with `--features xz` to be able to extract this archive"))
  }
}

#[cfg_attr(not(feature = "zstd"), allow(unused_variables))]
fn extract_tar_zst(file: &File, folder: &Path) -> Result<(), ExtractSpecificFormatError> {
  #[cfg(feature = "zstd")]
  {
    let zstd_decoder = zstd::Decoder::new(file).map_err(|error| ExtractSpecificFormatError::Other { format: ArchiveFormat::TarZst, error })?;
    let mut tar_decoder = tar::Archive::new(zstd_decoder);
    tar_decoder.unpack(folder).map_err(|error| ExtractSpecificFormatError::Other { format: ArchiveFormat::TarZst, error })
  }
  
  #[cfg(not(feature = "zstd"))]
  {
    Err(format!("This binary was built without Zstd support. Recompile with `--features zstd` to be able to extract this archive"))
  }
}
