use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use crate::itch_api_types::Game;
use crate::GamePlatform;

impl GamePlatform {
  fn get_allowed_extensions(&self) -> &'static [&'static str] {
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

  fn get_best_filenames(&self) -> &'static [&'static str] {
    match self {
      GamePlatform::Linux => &[],
      GamePlatform::Windows => &[],
      GamePlatform::OSX => &[],
      GamePlatform::Android => &[],
      GamePlatform::Web => &["index"],
      GamePlatform::Flash => &[],
      GamePlatform::Java => &[],
      GamePlatform::UnityWebPlayer => &[],
    }
  }
}

const GOOD_LAUNCH_FILENAMES: &[&'static str] = &["start", "launch", "play", "run", "game", "launcher"];

pub fn get_game_executable(upload_folder: &Path, platform: &GamePlatform, game_info: &Game) -> Result<Option<PathBuf>, String> {
  let mut best_executable: (Option<PathBuf>, i64) = (None, i64::MIN);

  for entry in WalkDir::new(upload_folder) {
    let entry = entry.map_err(|e| format!("Couldn't walk directory: {e}"))?;

    if entry.file_type().is_dir() {
      continue;
    }

    let rating = rate_executable(entry.path(), upload_folder, platform, game_info)?;
    if rating > best_executable.1 {
      best_executable = (Some(entry.into_path()), rating);
    }
  }

  Ok(best_executable.0)
}

fn rate_executable(file_path: &Path, upload_folder: &Path, platform: &GamePlatform, game_info: &Game) -> Result<i64, String> {
  let mut rating: i64 = 0;

  // If this is higher, that means the file is further away from the original folder, so it is worse
  let directory_levels: i64 = get_directory_difference(upload_folder,file_path)? - 1;
  assert!(directory_levels >= 0);
  // base level: keep the rating
  // 1st level: lower it by 1000
  // 2nd level: lower it by 4000
  // saturating_pow so it doesn't overflow
  rating -= directory_levels.saturating_pow(2) * 1000;

  // If the directory level is 3th or higher, exit now: we're not finding the executable that deep
  if directory_levels >= 3 {
    return Ok(rating);
  }

  // Most of the checks will be based on the filename
  let filename: String = make_alphanumeric_lowercase(
    file_path.file_stem()
      .expect("File doesn't have a filename????")
      .to_string_lossy()
      .to_string()
  );

  let extension = make_alphanumeric_lowercase(
    file_path.extension()
      .map(|s| s.to_string_lossy().to_string())
      .unwrap_or_default()
  );
  // If the file doesn't have an allowed extension, lower the rating by A LOT
  if !platform.get_allowed_extensions().iter().any(|ext| extension.eq_ignore_ascii_case(ext)) {
    rating -= 10000000;
  }
  // If the file has an ideal filename (e.g: index.html for a web game), raise the rating
  if platform.get_best_filenames().iter().any(|filen| filename.eq_ignore_ascii_case(filen)) {
    rating += 2300;
  }
  
  let game_title = make_alphanumeric_lowercase(game_info.title.clone());
  // Check if the filename is similar to the game name
  if strsim::levenshtein(game_title.as_str(), filename.as_str()) <= 1 {
    rating += 1200;
  }

  // If the filename has a good name (e.g: start, launch, etc.), raise the rating
  for n in GOOD_LAUNCH_FILENAMES {
    if strsim::levenshtein(n, filename.as_str()) <= 1 {
      rating += 1600;
    } else if filename.contains(n) {
      rating += 770;
    }
  }

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
  Ok(get_directory_level(&dst)? as i64 - get_directory_level(&src)? as i64)
}
