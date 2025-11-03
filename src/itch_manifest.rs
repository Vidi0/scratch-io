use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MANIFEST_FILENAME: &str = ".itch.toml";
const MANIFEST_PLAY_ACTION: &str = "play";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionPlatform {
  Linux,
  Windows,
  Osx,
  Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
  pub name: String,
  pub path: String,
  pub platform: Option<ActionPlatform>,
  pub args: Option<Vec<String>>,
  pub sandbox: Option<bool>,
  pub console: Option<bool>,
}

impl Action {
  pub fn get_canonical_path(&self, folder: &Path) -> Result<PathBuf, String> {
    folder.join(&self.path).canonicalize().map_err(|e| {
      format!(
        "Error getting the canonical form of the action path! Maybe it doesn't exist: {}\n{e}",
        self.path
      )
    })
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PrerequisiteName {
  #[serde(rename = "vcredist-2010-x64")]
  Vcredist2010x64,
  #[serde(rename = "vcredist-2010-x86")]
  Vcredist2010x86,
  #[serde(rename = "vcredist-2013-x64")]
  Vcredist2013x64,
  #[serde(rename = "vcredist-2013-x86")]
  Vcredist2013x86,
  #[serde(rename = "vcredist-2015-x64")]
  Vcredist2015x64,
  #[serde(rename = "vcredist-2015-x86")]
  Vcredist2015x86,
  #[serde(rename = "vcredist-2017-x64")]
  Vcredist2017x64,
  #[serde(rename = "vcredist-2017-x86")]
  Vcredist2017x86,
  #[serde(rename = "vcredist-2019-x64")]
  Vcredist2019x64,
  #[serde(rename = "vcredist-2019-x86")]
  Vcredist2019x86,

  #[serde(rename = "net-4.5.2")]
  Net452,
  #[serde(rename = "net-4.6")]
  Net46,
  #[serde(rename = "net-4.6.2")]
  Net462,

  #[serde(rename = "xna-4.0")]
  Xna40,

  #[serde(rename = "dx-june-2010")]
  DxJune2010,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Prerequisite {
  pub name: PrerequisiteName,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
  pub actions: Vec<Action>,
  pub prereqs: Option<Vec<Prerequisite>>,
}

/// Read the manifest from a folder and parse it (if any)
pub async fn read_manifest(upload_folder: &Path) -> Result<Option<Manifest>, String> {
  let manifest_path = upload_folder.join(MANIFEST_FILENAME);

  if !manifest_path.is_file() {
    return Ok(None);
  }

  let manifest_text: String = tokio::fs::read_to_string(&manifest_path)
    .await
    .map_err(|e| e.to_string())?;

  toml::from_str::<Manifest>(&manifest_text)
    .map(Some)
    .map_err(|e| {
      format!(
        "Couldn't parse itch manifest: {}\n{e}",
        manifest_path.to_string_lossy()
      )
    })
}

/// Returns a itch Manifest Action given its name and the folder where the game manifest is located
pub async fn launch_action(
  upload_folder: &Path,
  action_name: Option<&str>,
) -> Result<Option<Action>, String> {
  let Some(manifest) = read_manifest(upload_folder).await? else {
    return Ok(None);
  };

  let action_name = action_name.unwrap_or(MANIFEST_PLAY_ACTION);

  Ok(manifest.actions.into_iter().find(|a| a.name == action_name))
}
