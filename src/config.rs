use crate::eprintln_exit;
use scratch_io::InstalledUpload;

use std::{collections::HashMap};
use std::path::PathBuf;
use directories::ProjectDirs;
use serde::{Serialize, Deserialize};
use serde_with::{serde_as, DisplayFromStr};
use thiserror::Error;

const APP_CONFIGURATION_NAME: &str = "scratch-io";
const APP_CONFIGURATION_FILE: &str = "config.toml";
const LAST_CONFIGURATION_VERSION: u64 = 0;

/// Gets the config folder of this application
/// 
/// If `custom_config_folder` is provided, then use that path
fn get_config_folder(custom_config_folder: Option<PathBuf>) -> Option<ProjectDirs> {
  match custom_config_folder {
    None => ProjectDirs::from("", "", APP_CONFIGURATION_NAME),
    Some(p) => ProjectDirs::from_path(p)
  }
}

/// Gets the config file of this application
/// 
/// If `custom_config_folder` is provided, then use it as the config folder path instead of the system's default
fn get_config_file(custom_config_folder: Option<PathBuf>) -> Option<PathBuf> {
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

#[derive(Error, Debug)]
pub enum LoadConfigError {
  #[error("Couldn't determine the config file!")]
  CouldntGetConfigFile,

  #[error("Couldn't check if the path exists: \"{path}\"\n{error}")]
  CouldntCheckIfPathExists {
    path: PathBuf,
    #[source]
    error: std::io::Error,
  },

  #[error("Couldn't read the config file to a string: \"{path}\"\n{error}")]
  CouldntReadFileToString {
    path: PathBuf,
    #[source]
    error: std::io::Error,
  },

  #[error("Couldn't get the config version: \"{path}\"\n{error}\n\n{text}")]
  InvalidConfigVersion {
    path: PathBuf,
    text: String,
    #[source]
    error: toml::de::Error,
  },

  #[error("Invalid configuration file: \"{path}\"\n{error}\n\n{text}")]
  InvalidConfigFile {
    path: PathBuf,
    text: String,
    #[source]
    error: toml::de::Error,
  },

  #[error(
r#"The config version of "{config_file}" is not compatible with this scratch-io version!
Update to a newer scratch-io version to be able to load the given config.
  Config version: {config_version}
  Supported version: {supported_version}"#
)]
  IncompatibleConfigVersion {
    config_file: PathBuf,
    config_version: u64,
    supported_version: u64,
  },
}

#[derive(Error, Debug)]
pub enum SaveConfigError {
  #[error("Couldn't write the config string to a file: \"{path}\"\n{error}\n\n{text}")]
  CouldntWriteStringToFile {
    path: PathBuf,
    text: String,
    #[source]
    error: std::io::Error,
  },

  #[error("Couldn't determine the config file!")]
  CouldntGetConfigFile,
  
  #[error("Couldn't serialize config into TOML: \"{path}\"\n{error}")]
  SerializeConfig {
    #[source]
    error: toml::ser::Error,
    path: PathBuf,
  },
}

impl Config {
  /// Load the application's config from a file
  /// 
  /// If `custom_config_folder` is provided, then use that as the config folder path instead of the system's default
  pub async fn load(custom_config_folder: Option<PathBuf>) -> Result<Self, LoadConfigError> {
    // Get the config path
    let config_file_path: PathBuf = get_config_file(custom_config_folder)
      .ok_or(LoadConfigError::CouldntGetConfigFile)?;

    // If the config doesn't exist, create one with Config::default()
    if !config_file_path.try_exists().map_err(|error| LoadConfigError::CouldntCheckIfPathExists { path: config_file_path.to_path_buf(), error })? {
      return Ok(Config::default());
    }

    // Get the config text
    let config_text: String = tokio::fs::read_to_string(&config_file_path).await
      .map_err(|error| LoadConfigError::CouldntReadFileToString { path: config_file_path.to_path_buf(), error })?;

    // Get the config version
    let ver = toml::from_str::<ConfigVersion>(&config_text)
      .map_err(|error| LoadConfigError::InvalidConfigVersion { path: config_file_path.to_path_buf(), text: config_text.clone(), error })?
      .config_version;

    // Parse the config depending on the version
    match ver {
      LAST_CONFIGURATION_VERSION => toml::from_str::<Config>(&config_text)
        .map_err(|error| LoadConfigError::InvalidConfigFile { path: config_file_path, text: config_text, error }),
      _ => Err(LoadConfigError::IncompatibleConfigVersion { config_file: config_file_path, config_version: ver, supported_version: LAST_CONFIGURATION_VERSION })
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
  pub async fn save(&self, custom_config_folder: Option<PathBuf>) -> Result<(), SaveConfigError> {
    // Get the config path
    let config_file_path: PathBuf = get_config_file(custom_config_folder)
      .ok_or(SaveConfigError::CouldntGetConfigFile)?;

    // Get the config text
    let config_text = toml::to_string_pretty::<Config>(self)
      .map_err(|error| SaveConfigError::SerializeConfig { error, path: config_file_path.to_path_buf() })?;

    // Write the config to a file
    tokio::fs::write(&config_file_path, &config_text).await
      .map_err(|error| SaveConfigError::CouldntWriteStringToFile { path: config_file_path, text: config_text, error })
  }
  
  /// Save the application's config to a file and panic on error
  /// 
  /// If `custom_config_folder` is provided, then use that as the config folder path instead of the system's default
  pub async fn save_unwrap(&self, custom_config_folder: Option<PathBuf>) {
    self.save(custom_config_folder).await.unwrap_or_else(|e| eprintln_exit!("Error while saving to the configuration file!\n{}", e))
  }
}
