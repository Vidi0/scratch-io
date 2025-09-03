use std::path::{Path, PathBuf};
use std::fs::{File};
use crate::game_files_operations::*;

/// Checks if the folder where the archive will be extracted is empty
/// 
/// Returns the folder where the files will be extracted and if it is empty or not
fn is_extraction_folder_empty(file_path: &Path, format: &ArchiveFormat) -> Result<(bool, PathBuf), String> {
  if let ArchiveFormat::Other = format {
    return Ok((true, file_path.to_path_buf()));
  }

  let folder = file_path
    .parent()
    .expect(format!("Couldn't get the parent of the archive: \"{}\"", file_path.to_string_lossy()).as_str())
    .join(file_without_extension(file_path).expect("File doesn't have an extension?"));

  // If the directory exists and isn't empty, return an error
  if !is_folder_empty(&folder)? {
    return Ok((false, folder));
  }

  Ok((true, folder))
}


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
fn get_archive_format(file: &Path) -> ArchiveFormat {
  let Some(extension) = file.extension().map(|e| e.to_string_lossy()) else {
    return ArchiveFormat::Other
  };

  // At this point, we know the file has an extension
  let is_tar_compressed: bool = get_file_stem(file).expect("File doesn't have an extension?")
    .to_lowercase()
    .ends_with(".tar");

  if extension.eq_ignore_ascii_case("zip") {
    ArchiveFormat::Zip
  } else if extension.eq_ignore_ascii_case("tar") {
    ArchiveFormat::Tar
  } else if is_tar_compressed && extension.eq_ignore_ascii_case("gz")
    || extension.eq_ignore_ascii_case("tgz")
    || extension.eq_ignore_ascii_case("taz") {
    ArchiveFormat::TarGz
  } else if is_tar_compressed && extension.eq_ignore_ascii_case("bz2")
    || extension.eq_ignore_ascii_case("tbz")
    || extension.eq_ignore_ascii_case("tbz2")
    || extension.eq_ignore_ascii_case("tz2") {
    ArchiveFormat::TarBz2
  } else if is_tar_compressed && extension.eq_ignore_ascii_case("xz")
    || extension.eq_ignore_ascii_case("txz") {
    ArchiveFormat::TarXz
  } else if is_tar_compressed && extension.eq_ignore_ascii_case("zst")
    || extension.eq_ignore_ascii_case("tzst") {
    ArchiveFormat::TarZst
  } else {
    ArchiveFormat::Other
  }
}

/// Extracts the archive into a folder with the same name (without the extension)
/// 
/// This function can return a path to a file (if it's not a valid archive) or to the extracted folder
pub async fn extract(file_path: &Path) -> Result<(), String> {
  let format: ArchiveFormat = get_archive_format(file_path);

  // If the file isn't an archive, return now
  if let ArchiveFormat::Other = format {
    crate::make_executable(file_path)?;
    return Ok(());
  }

  let parent_folder = file_path.parent()
    .expect(format!("Couldn't get the parent of the archive: \"{}\"", file_path.to_string_lossy()).as_str());

  let extract_folder = is_extraction_folder_empty(file_path, &format).and_then(|(is_empty, folder)| {
    if is_empty {
      Ok(folder)
    } else {
      Err(format!("The folder where the archive will be extracted isn't empty!: \"{}\"", folder.to_string_lossy()))
    }
  })?;

  let file = File::open(file_path)
    .map_err(|e| e.to_string())?;

  match format {
    ArchiveFormat::Other => {
      panic!("If the format is Other, we should've exited before!");
    }
    ArchiveFormat::Zip => {
      extract_zip(&file, &extract_folder)?;
    }
    ArchiveFormat::Tar => {
      extract_tar(&file, &extract_folder)?;
    }
    ArchiveFormat::TarGz => {
      extract_tar_gz(&file, &extract_folder)?;
    }
    ArchiveFormat::TarBz2 => {
      extract_tar_bz2(&file, &extract_folder)?;
    }
    ArchiveFormat::TarXz => {
      extract_tar_xz(&file, &extract_folder)?;
    }
    ArchiveFormat::TarZst => {
      extract_tar_zst(&file, &extract_folder)?;
    }
  }

  // Remove the archive
  tokio::fs::remove_file(file_path).await
    .map_err(|e| e.to_string())?;

  // Move all the contents of the extrated file to the upload folder
  remove_root_folder(&parent_folder)?;

  Ok(())
}

#[cfg_attr(not(feature = "zip"), allow(unused_variables))]
fn extract_zip(file: &File, folder: &Path) -> Result<(), String> {
  #[cfg(feature = "zip")] 
  {
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    archive.extract(&folder).map_err(|e| format!("Error extracting ZIP archive: {e}"))
  }

  #[cfg(not(feature = "zip"))]
  {
    Err(format!("This binary was built without ZIP support. Recompile with `--features zip` to be able to extract this archive"))
  }
}

#[cfg_attr(not(feature = "tar"), allow(unused_variables))]
fn extract_tar(file: &File, folder: &Path) -> Result<(), String> {
  #[cfg(feature = "tar")]
  {
    let mut tar_decoder = tar::Archive::new(file);
    tar_decoder.unpack(&folder).map_err(|e| format!("Error extracting tar archive: {e}"))
  }

  #[cfg(not(feature = "tar"))]
  {
    Err(format!("This binary was built without TAR support. Recompile with `--features tar` to be able to extract this archive"))
  }
}

#[cfg_attr(not(feature = "gzip"), allow(unused_variables))]
fn extract_tar_gz(file: &File, folder: &Path) -> Result<(), String> {
  #[cfg(feature = "gzip")]
  {
    let gz_decoder = flate2::read::GzDecoder::new(file);
    let mut tar_decoder = tar::Archive::new(gz_decoder);
    tar_decoder.unpack(&folder).map_err(|e| format!("Error extracting tar.gz archive: {e}"))
  }

  #[cfg(not(feature = "gzip"))]
  {
    Err(format!("This binary was built without gzip support. Recompile with `--features gzip` to be able to extract this archive"))
  }
}

#[cfg_attr(not(feature = "bzip2"), allow(unused_variables))]
fn extract_tar_bz2(file: &File, folder: &Path) -> Result<(), String> {
  #[cfg(feature = "bzip2")]
  {
    let bz2_decoder = bzip2::read::BzDecoder::new(file);
    let mut tar_decoder = tar::Archive::new(bz2_decoder);
    tar_decoder.unpack(&folder).map_err(|e| format!("Error extracting tar.gz archive: {e}"))
  }

  #[cfg(not(feature = "bzip2"))]
  {
    Err(format!("This binary was built without bzip2 support. Recompile with `--features bzip2` to be able to extract this archive"))
  }
}

#[cfg_attr(not(feature = "xz"), allow(unused_variables))]
fn extract_tar_xz(file: &File, folder: &Path) -> Result<(), String> {
  #[cfg(feature = "xz")]
  {
    let xz_decoder = liblzma::read::XzDecoder::new(file);
    let mut tar_decoder = tar::Archive::new(xz_decoder);
    tar_decoder.unpack(&folder).map_err(|e| format!("Error extracting tar.xz archive: {e}"))
  }
  
  #[cfg(not(feature = "xz"))]
  {
    Err(format!("This binary was built without XZ support. Recompile with `--features xz` to be able to extract this archive"))
  }
}

#[cfg_attr(not(feature = "zstd"), allow(unused_variables))]
fn extract_tar_zst(file: &File, folder: &Path) -> Result<(), String> {
  #[cfg(feature = "zstd")]
  {
    let zstd_decoder = zstd::Decoder::new(file).map_err(|e| format!("Error reading tar.zst archive: {e}"))?;
    let mut tar_decoder = tar::Archive::new(zstd_decoder);
    tar_decoder.unpack(&folder).map_err(|e| format!("Error extracting tar.zst archive: {e}"))
  }
  
  #[cfg(not(feature = "zstd"))]
  {
    Err(format!("This binary was built without Zstd support. Recompile with `--features zstd` to be able to extract this archive"))
  }
}
