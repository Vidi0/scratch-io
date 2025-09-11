use std::path::{Path, PathBuf};
use std::fs;
use serde::Deserialize;

const MANIFEST_FILENAME: &str = ".itch.toml";
const MANIFEST_PLAY_ACTION: &str = "play";

#[derive(Deserialize)]
pub struct Action {
  pub name: String,
  pub path: String,
  pub args: Option<Vec<String>>,
}

impl std::fmt::Display for Action {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f,
"    Name: {}
      Path: {}
      Arguments: {}",
      self.name,
      self.path,
      self.args.as_ref().unwrap_or(&Vec::new()).iter().map(|a| format!("\"{a}\"")).collect::<Vec<String>>().join(", "),
    )
  }
}

impl Action {
  pub fn get_canonical_path(&self, folder: &Path) -> Result<PathBuf, String> {
    folder.join(&self.path)
      .canonicalize()
      .map_err(|e| format!("Error getting the canonical form of the action path! Maybe it doesn't exist: {}\n{e}", self.path))
  }
}

#[derive(Deserialize)]
pub struct Manifest {
  pub actions: Vec<Action>,
}

impl std::fmt::Display for Manifest {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f,
"  Manifest actions:
{}",
      self.actions.iter().map(|a| a.to_string()).collect::<Vec<String>>().join("\n"),
    )
  }
}

/// Read the manifest from a folder and parse it (if any)
pub fn read_manifest(upload_folder: &Path) -> Result<Option<Manifest>, String> {
  let manifest_path = upload_folder.join(MANIFEST_FILENAME);

  if !manifest_path.is_file() {
    return Ok(None);
  }

  let manifest_text: String = fs::read_to_string(&manifest_path)
    .map_err(|e| e.to_string())?;

  toml::from_str::<Manifest>(&manifest_text)
    .map(|m| Some(m))
    .map_err(|e| format!("Couldn't parse itch manifest: {}\n{e}", manifest_path.as_path().to_string_lossy()))
}

/// Returns a Itch Manifest Action given its name and the folder where the game manifest is located
pub fn launch_action(upload_folder: &Path, action_name: Option<&str>) -> Result<Option<Action>, String> {
  let Some(manifest) = read_manifest(upload_folder)? else {
    return Ok(None);
  };

  let action_name = action_name.unwrap_or(MANIFEST_PLAY_ACTION);

  Ok(
    manifest.actions.into_iter()
      .find(|a| a.name == action_name)
  )
}
