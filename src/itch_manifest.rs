use std::path::{Path, PathBuf};
use std::fs;
use serde::{Serialize, Deserialize};
use crate::error::*;

const MANIFEST_FILENAME: &str = ".itch.toml";
const MANIFEST_PLAY_ACTION: &str = "play";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
  pub name: String,
  pub path: String,
  pub args: Option<Vec<String>>,
}

impl Action {
  pub fn get_canonical_path(&self, folder: &Path) -> Result<PathBuf> {
    let path = folder.join(&self.path);
    
    path.canonicalize()
      .map_err(|error| FilesystemError::GetCanonicalPath { error, path }.into())
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
  pub actions: Vec<Action>,
}

/// Read the manifest from a folder and parse it (if any)
pub fn read_manifest(upload_folder: &Path) -> Result<Option<Manifest>> {
  let manifest_path = upload_folder.join(MANIFEST_FILENAME);

  if !manifest_path.is_file() {
    return Ok(None);
  }

  let manifest_text: String = fs::read_to_string(&manifest_path)
    .map_err(|error| FilesystemError::ReadFileToString { error, path: manifest_path.to_path_buf() })?;

  toml::from_str::<Manifest>(&manifest_text)
    .map(Some)
    .map_err(|error| ParseError::ItchManifest { error, path: manifest_path, text: manifest_text }.into())
}

/// Returns a itch Manifest Action given its name and the folder where the game manifest is located
pub fn launch_action(upload_folder: &Path, action_name: Option<&str>) -> Result<Option<Action>> {
  let Some(manifest) = read_manifest(upload_folder)? else {
    return Ok(None);
  };

  let action_name = action_name.unwrap_or(MANIFEST_PLAY_ACTION);

  Ok(
    manifest.actions.into_iter()
      .find(|a| a.name == action_name)
  )
}
