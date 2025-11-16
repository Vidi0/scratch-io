use crate::{GamePlatform, errors::FilesystemError, filesystem};
use std::path::{Path, PathBuf};

const GOOD_LAUNCH_FILENAMES: &[&str] = &[
  "start", "launch", "play", "run", "game", "launcher", "rungame",
];
const ARCHITECTURE_SUFFIXES: &[&str] = {
  #[cfg(target_pointer_width = "64")]
  {
    &["64"]
  }
  #[cfg(not(target_pointer_width = "64"))]
  {
    &["32"]
  }
};
// This multiplier has to be between 0 and 1
const BEST_PROXIMITY_MULTIPLIER: f64 = 0.34;
// If the level is 3 or more, stop searching the executable
const MAX_DIRECTORY_LEVEL_DEPTH: usize = 2;

impl GamePlatform {
  fn get_allowed_extensions(&self) -> &'static [&'static str] {
    // These must only be ascii alphanumeric lowercase
    match self {
      GamePlatform::Linux => &["x8664", "x86", "bin", "sh", "run", ""],
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
      // These must only be ascii alphanumeric lowercase
      GamePlatform::Linux
      | GamePlatform::Windows
      | GamePlatform::OSX
      | GamePlatform::Android
      | GamePlatform::Flash
      | GamePlatform::Java
      | GamePlatform::UnityWebPlayer => &[],
      GamePlatform::Web => &["index"],
    }
  }
}

/// Tries to get the best game executable based on some data
///
/// # Arguments
///
/// * `upload_folder` - The folder where the search will be done
///
/// * `platform` - The platform the game executable will be run on
///
/// * `game_info` - Information about the game
///
/// # Returns
///
/// A path with the best guess for the game executable
///
/// An error if something goes wrong
pub async fn get_game_executable(
  upload_folder: &Path,
  platform: GamePlatform,
  game_title: String,
) -> Result<PathBuf, String> {
  // If the folder is not a directory, return
  filesystem::ensure_is_dir(upload_folder).await?;

  // This variable will store the best executable found at the moment and its rating
  let mut best_executable: (Option<PathBuf>, i64) = (None, i64::MIN);

  // We will add the folders and their depth to this VecDeque
  let mut queue: std::collections::VecDeque<(PathBuf, usize)> = std::collections::VecDeque::new();
  queue.push_back((upload_folder.to_path_buf(), 0));

  while let Some((folder, depth)) = queue.pop_front() {
    let mut entries = filesystem::read_dir(&folder).await?;

    while let Some(entry) = filesystem::next_entry(&mut entries, &folder).await? {
      let entry_path = entry.path();

      if filesystem::file_type(&entry, &folder).await?.is_dir() {
        // If we are on the last depth, don't go to the next one, stop now
        // For this reason it is < and not <=
        if depth < MAX_DIRECTORY_LEVEL_DEPTH {
          queue.push_back((entry_path, depth + 1));
        }
      } else {
        let rating = rate_executable(&entry_path, depth, platform, &game_title)?;
        if rating > best_executable.1 {
          best_executable = (Some(entry_path), rating);
        }
      }
    }
  }

  if let Some(executable) = best_executable.0 {
    Ok(executable)
  } else {
    Err(format!(
      "Couldn't find any game file executable in: \"{}\"",
      upload_folder.to_string_lossy()
    ))
  }
}

/// Rate the probability that a given path is the main executable file of a game.
///
/// # Arguments
///
/// * `file_path` - The path to rate
///
/// * `directory_levels` - The number of directory levels between the path and the root folder of the game
///
/// * `platform` - The platform the game executable will be run on
///
/// * `game_title` - Information about the game
///
/// # Returns
///
/// The rating
///
/// An error if something goes wrong
fn rate_executable(
  file_path: &Path,
  directory_levels: usize,
  platform: GamePlatform,
  game_title: &str,
) -> Result<i64, FilesystemError> {
  let mut rating: i64 = 0;

  // base level: keep the rating
  // 1st level: lower it by 1000
  // 2nd level: lower it by 4000
  // saturating_pow so it doesn't overflow
  assert!(directory_levels <= MAX_DIRECTORY_LEVEL_DEPTH);
  rating -= (directory_levels as i64).saturating_pow(2) * 1000;

  // Most of the checks will be based on the filename
  let filename: String =
    make_alphanumeric_lowercase(filesystem::get_file_stem(file_path)?.to_owned());

  let extension = make_alphanumeric_lowercase(
    file_path
      .extension()
      .map(|s| s.to_string_lossy().to_string())
      .unwrap_or_default(),
  );
  // If the file doesn't have an allowed extension, lower the rating by A LOT
  if !platform
    .get_allowed_extensions()
    .iter()
    .any(|ext| extension.eq_ignore_ascii_case(ext))
  {
    rating -= 10_000_000;
  }

  // If the file has an ideal filename (e.g: index.html for a web game), raise the rating
  if platform
    .get_best_filenames()
    .iter()
    .any(|filen| filename.eq_ignore_ascii_case(filen))
  {
    rating += 2300;
  }

  // Check if the filename is similar to the game name or the game name with cerating suffixes
  rating += proximity_rating_with_suffixes(
    game_title,
    &filename,
    ARCHITECTURE_SUFFIXES,
    2,
    700,
    550,
    BEST_PROXIMITY_MULTIPLIER,
  );

  // If the filename has a good name (e.g: start, launch, etc.), raise the rating
  for n in GOOD_LAUNCH_FILENAMES {
    rating += proximity_rating(n, &filename, 1, 1200, 500, BEST_PROXIMITY_MULTIPLIER);
  }

  Ok(rating)
}

fn make_alphanumeric_lowercase(mut string: String) -> String {
  string.retain(|c| c.is_ascii_alphanumeric());
  string.make_ascii_lowercase();
  string
}

fn proximity_rating(
  a: &str,
  b: &str,
  max_distance: usize,
  base_points: i64,
  extra_points: i64,
  proximity_multiplier: f64,
) -> i64 {
  if strsim::levenshtein(a, b) >= max_distance {
    return 0;
  }

  // The matematical equation for the extra points is:
  // y = normalized ^ ( 1 / (multiplier ^ 2) )
  // This works when multiplier is between 0 and 1
  base_points
    + (strsim::normalized_levenshtein(a, b).powf(1.0 / proximity_multiplier.powf(2.0))
      * extra_points as f64) as i64
}

fn proximity_rating_with_suffixes(
  a: &str,
  b: &str,
  suffixes: &[&str],
  max_distance: usize,
  base_points: i64,
  extra_points: i64,
  proximity_multiplier: f64,
) -> i64 {
  let mut rating: i64 = 0;

  // Iterate over the original string and the string appending the suffixes
  // For that reason a empty string is chained, to iterate over the original string as well
  for e in suffixes.iter().chain(std::iter::once(&"")) {
    // Only the best rating will be kept, not all of them, for that reason max() is used
    rating = rating.max(proximity_rating(
      a,
      &format!("{b}{e}"),
      max_distance,
      base_points,
      extra_points,
      proximity_multiplier,
    ));
  }

  rating
}
