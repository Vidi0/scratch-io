use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use crate::itch_api_types::{Game, Upload};
use crate::GamePlatform;

impl GamePlatform {
  pub fn get_allowed_extensions(&self) -> &'static [&'static str] {
    match self {
      GamePlatform::Linux => &["x86_64", "x86", "bin", "sh", "run", ""],
      GamePlatform::Windows => &["exe", "msi", "bat"],
      GamePlatform::OSX => &["dmg", "app", "pkg"],
      GamePlatform::Android => &["apk"],
      GamePlatform::Web => &["html"],
      GamePlatform::Flash => &["swf"],
      GamePlatform::Java => &["jar"],
      GamePlatform::UnityWebPlayer => &["unity3d"],
    }
  }
}

pub fn get_game_executable(upload_folder: &Path, platform: &GamePlatform, game_info: &Game, upload_info: &Upload) -> Result<Option<PathBuf>, String> {
  let mut best_executable: (Option<PathBuf>, i64) = (None, i64::MIN);

  for entry in WalkDir::new(upload_folder) {
    let entry_path = entry.map_err(|e| format!("Couldn't walk directory: {e}"))?.into_path();
    let extension = entry_path.extension().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();

    // If the file doesn't have an allowed extension, continue
    if !platform.get_allowed_extensions().iter().any(|ext| extension.eq_ignore_ascii_case(ext)) {
      continue;
    }

    let rating = rate_executable(entry_path.as_path(), upload_folder, game_info, upload_info)?;
    if rating > best_executable.1 {
      best_executable = (Some(entry_path), rating);
    }
  }

  Ok(best_executable.0)
}

fn rate_executable(file_path: &Path, upload_folder: &Path, game_info: &Game, upload_info: &Upload) -> Result<i64, String> {
  let mut rating: i64 = 0;

  // If this is higher, that means the file is further away from the original folder, so it is worse
  let directory_levels: i64 = get_directory_difference(upload_folder,file_path)? - 1;
  assert!(directory_levels >= 0);
  rating -= directory_levels * 1000;

  // Most of the checks will be based on the filename
  let filename: String = make_alphanumeric_lowercase(
    file_path.file_stem()
      .expect("File doesn't have a filename????")
      .to_string_lossy()
      .to_string()
  );
  
  let game_title = make_alphanumeric_lowercase(game_info.title.clone());

  todo!();

  Ok(rating)
}

fn make_alphanumeric_lowercase(mut string: String) -> String {
  string.retain(|c| c.is_ascii_alphanumeric());
  string.make_ascii_lowercase();
  string
}

fn get_directory_level<P: AsRef<Path>>(path: P) -> Result<usize, String> {
  path.as_ref()
    .canonicalize()
    .map_err(|e| format!("Error getting the canonical form of the path!: {e}"))
    .map(|p| p.components().count())
}

fn get_directory_difference<P: AsRef<Path>>(src: P, dst: P) -> Result<i64, String> {
  Ok(get_directory_level(dst)? as i64 - get_directory_level(src)? as i64)
}
