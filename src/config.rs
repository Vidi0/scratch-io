use std::{collections::HashMap};
use std::path::PathBuf;
use directories::ProjectDirs;
use serde::{Serialize, Deserialize};
use serde_with::{serde_as, DisplayFromStr};
use scratch_io::InstalledUpload;
use scratch_io::error::*;
use crate::eprintln_exit;

const APP_CONFIGURATION_NAME: &str = "scratch-io";
const APP_CONFIGURATION_FILE: &str = "config.toml";
const LAST_CONFIGURATION_VERSION: u64 = 0;

/// Gets the config folder of this application
/// 
/// If `custom_config_folder` is provided, then use that path
fn get_config_folder(custom_config_folder: Option<PathBuf>) -> Result<ProjectDirs> {
  match custom_config_folder {
    None => ProjectDirs::from("", "", APP_CONFIGURATION_NAME),
    Some(p) => ProjectDirs::from_path(p)
  }.ok_or_else(|| FilesystemError::MissingProjectDirectory.into())
}

/// Gets the config file of this application
/// 
/// If `custom_config_folder` is provided, then use it as the config folder path instead of the system's default
fn get_config_file(custom_config_folder: Option<PathBuf>) -> Result<PathBuf> {
  get_config_folder(custom_config_folder).map(|d| d.config_dir()
    .join(APP_CONFIGURATION_FILE)
  )
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
  pub installed_uploads: HashMap<u64, InstalledUpload>,
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
    if !config_file_path.try_exists().map_err(|error| FilesystemError::CheckIfPathExists { error, path: config_file_path.to_path_buf() })? {
      return Ok(Config::default());
    }

    // Get the config text
    let config_text: String = tokio::fs::read_to_string(&config_file_path).await
      .map_err(|error| FilesystemError::ReadFileToString { error, path: config_file_path.to_path_buf() })?;

    // Get the config version
    let ver = toml::from_str::<ConfigVersion>(&config_text)
      .map_err(|error| ParseError::ConfigVersion { error, path: config_file_path.to_path_buf(), text: config_text.clone() })?
      .config_version;

    // Parse the config depending on the version
    match ver {
      LAST_CONFIGURATION_VERSION => toml::from_str::<Config>(&config_text)
        .map_err(|error| ParseError::ConfigFile { error, path: config_file_path, text: config_text }.into()),
      _ => Err(ErrorKind::IncompatibleConfigVersion { config_file: config_file_path, config_version: ver, supported_version: LAST_CONFIGURATION_VERSION }.into())
    }
  }
  
  /// Load the application's config from a file and panic on error
  /// 
  /// If `custom_config_folder` is provided, then use that as the config folder path instead of the system's default
  pub async fn load_unwrap(custom_config_folder: Option<PathBuf>) -> Self {
    Self::load(custom_config_folder).await.unwrap_or_else(|e| eprintln_exit!("Error while reading configuration file!\n{}", e))
  }
  
  /// Save the application's config to a file
  /// 
  /// If `custom_config_folder` is provided, then use that as the config folder path instead of the system's default
  pub async fn save(&self, custom_config_folder: Option<PathBuf>) -> Result<()> {
    // Get the config path
    let config_file_path: PathBuf = get_config_file(custom_config_folder)?;

    // Get the config text
    let config_text = toml::to_string_pretty::<Config>(self)
      .map_err(|error| ParseError::SerializeConfig { error, path: config_file_path.to_path_buf() })?;

    // Write the config to a file
    tokio::fs::write(&config_file_path, &config_text).await
      .map_err(|error| FilesystemError::WriteStringToFile { error, path: config_file_path }.into())
  }
  
  /// Save the application's config to a file and panic on error
  /// 
  /// If `custom_config_folder` is provided, then use that as the config folder path instead of the system's default
  pub async fn save_unwrap(&self, custom_config_folder: Option<PathBuf>) {
    self.save(custom_config_folder).await.unwrap_or_else(|e| eprintln_exit!("Error while saving to the configuration file!\n{}", e))
  }
}
