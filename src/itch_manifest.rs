use crate::{errors::FilesystemError, game_files_operations, itch_api::types::*};
use std::path::{Path, PathBuf};

const MANIFEST_FILENAME: &str = ".itch.toml";
const MANIFEST_PLAY_ACTION: &str = "play";

impl ManifestAction {
  pub async fn get_canonical_path(&self, folder: &Path) -> Result<PathBuf, FilesystemError> {
    game_files_operations::get_canonical_path(&folder.join(&self.path)).await
  }
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

/// Returns an itch.io ManifestAction given its name and the folder where the game manifest is located
pub async fn launch_action(
  upload_folder: &Path,
  action_name: Option<&str>,
) -> Result<Option<ManifestAction>, String> {
  let Some(manifest) = read_manifest(upload_folder).await? else {
    return Ok(None);
  };

  let action_name = action_name.unwrap_or(MANIFEST_PLAY_ACTION);

  Ok(
    manifest
      .actions
      .unwrap_or_default()
      .into_iter()
      .find(|a| a.name == action_name),
  )
}
