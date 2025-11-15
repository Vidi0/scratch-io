use anyhow::{Context, Result};
use directories::ProjectDirs;
use scratch_io::{InstalledUpload, itch_api::types::UploadID};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use std::collections::HashMap;
use std::path::PathBuf;

const APP_CONFIGURATION_NAME: &str = "scratch-io";
const APP_CONFIGURATION_FILE: &str = "config.toml";
const LAST_CONFIGURATION_VERSION: u64 = 0;

/// Gets the config folder of this application
///
/// If `custom_config_folder` is provided, then use that path
fn get_config_folder(custom_config_folder: Option<PathBuf>) -> Result<ProjectDirs> {
  match custom_config_folder {
    None => ProjectDirs::from("", "", APP_CONFIGURATION_NAME),
    Some(p) => ProjectDirs::from_path(p),
  }
  .with_context(|| "Couldn't determine the project directory!")
}

/// Gets the config file of this application
///
/// If `custom_config_folder` is provided, then use it as the config folder path instead of the system's default
fn get_config_file(custom_config_folder: Option<PathBuf>) -> Result<PathBuf> {
  get_config_folder(custom_config_folder).map(|d| d.config_dir().join(APP_CONFIGURATION_FILE))
}

/// A struct for deserializing the config version
///
/// After the config file is parsed into this struct, it will be parsed into
/// the corresponding Config's version
#[derive(Deserialize)]
struct ConfigVersion {
  config_version: u64,
}

/// The latest config version
#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct Config {
  pub config_version: u64,
  pub api_key: Option<String>,
  #[serde_as(as = "HashMap<DisplayFromStr, _>")]
  pub installed_uploads: HashMap<UploadID, InstalledUpload>,
}

impl std::default::Default for Config {
  fn default() -> Self {
    Self {
      config_version: LAST_CONFIGURATION_VERSION,
      api_key: None,
      installed_uploads: HashMap::new(),
    }
  }
}

impl Config {
  /// Load the application's config from a file
  ///
  /// If `custom_config_folder` is provided, then use that as the config folder path instead of the system's default
  pub async fn load(custom_config_folder: Option<PathBuf>) -> Result<Self> {
    // Get the config path
    let config_file_path: PathBuf = get_config_file(custom_config_folder)?;

    // If the config doesn't exist, create one with Config::default()
    if !config_file_path.try_exists().with_context(|| {
      format!(
        "Couldn't check if the config file exists: \"{}\"",
        config_file_path.to_string_lossy()
      )
    })? {
      return Ok(Config::default());
    }

    // Get the config text
    let config_text: String = tokio::fs::read_to_string(&config_file_path)
      .await
      .with_context(|| {
        format!(
          "Couldn't read the config file data: \"{}\"",
          config_file_path.to_string_lossy()
        )
      })?;

    // Get the config version
    let ver = toml::from_str::<ConfigVersion>(&config_text)
      .with_context(|| {
        format!(
          "Couldn't get the config version: \"{}\"",
          config_file_path.to_string_lossy()
        )
      })?
      .config_version;

    // Parse the config depending on the version
    match ver {
      LAST_CONFIGURATION_VERSION => toml::from_str::<Config>(&config_text),
      _ => {
        return Err(anyhow::Error::msg(format!(
          r#"The config version of "{}" is not compatible with this scratch-io version!
Update to a newer scratch-io version to be able to load the given config.
  Config version: {ver}
  Supported version: {LAST_CONFIGURATION_VERSION}"#,
          config_file_path.to_string_lossy()
        )));
      }
    }
    .with_context(|| {
      format!(
        "Invalid configuration file: \"{}\"",
        config_file_path.to_string_lossy()
      )
    })
  }

  /// Save the application's config to a file
  ///
  /// If `custom_config_folder` is provided, then use that as the config folder path instead of the system's default
  pub async fn save(&self, custom_config_folder: Option<PathBuf>) -> Result<()> {
    // Get the config path
    let config_file_path: PathBuf = get_config_file(custom_config_folder)?;

    // Get the config text
    let config_text = toml::to_string_pretty::<Config>(self)
      .with_context(|| "Couldn't serialize config into TOML!")?;

    // Ensure config directory exists
    if let Some(parent) = config_file_path.parent() {
      tokio::fs::create_dir_all(parent).await.with_context(|| {
        format!(
          "Couldn't create config directory: \"{}\"",
          parent.to_string_lossy()
        )
      })?;
    }

    // Write the config to a file
    tokio::fs::write(&config_file_path, &config_text)
      .await
      .with_context(|| {
        format!(
          "Couldn't write config to a file: \"{}\"",
          config_file_path.to_string_lossy()
        )
      })
  }
}
