use reqwest::Client;
use serde::{Serialize, Deserialize};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use scratch_io::{itch_types::*, DownloadStatus};

const APP_CONFIGURATION_NAME: &str = "scratch-io";
const APP_CONFIGURATION_FILE: &str = "config";

macro_rules! eprintln_exit {
  ($($arg:tt)*) => {{
    eprintln!($($arg)*);
    std::process::exit(1);
  }};
}

/// Serialize HashMap<u64, V> as a map with string keys.
pub fn serialize_u64_map<S, V>(
  map: &HashMap<u64, V>,
  serializer: S,
) -> Result<S::Ok, S::Error>
where
  S: serde::Serializer,
  V: Serialize,
{
  // Use an ordered map for deterministic output; use HashMap if you prefer.
  let mut tmp: std::collections::BTreeMap<String, &V> = std::collections::BTreeMap::new();
  for (k, v) in map {
    tmp.insert(k.to_string(), v);
  }
  tmp.serialize(serializer)
}

/// Deserialize a map with string keys into HashMap<u64, V>.
pub fn deserialize_u64_map<'de, D, V>(
  deserializer: D,
) -> Result<HashMap<u64, V>, D::Error>
where
  D: serde::Deserializer<'de>,
  V: Deserialize<'de>,
{
  let tmp: std::collections::BTreeMap<String, V> = std::collections::BTreeMap::deserialize(deserializer)?;
  let mut map = HashMap::with_capacity(tmp.len());
  for (k, v) in tmp {
    let num = k.parse::<u64>().map_err(|e| serde::de::Error::custom(format!("invalid u64 key `{}`: {}", k, e)))?;
    map.insert(num, v);
  }
  Ok(map)
}

#[derive(Serialize, Deserialize)]
struct Config {
  api_key: Option<String>,
  #[serde(
    serialize_with = "serialize_u64_map",
    deserialize_with = "deserialize_u64_map"
  )]
  installed_uploads: HashMap<u64, scratch_io::UploadInstallInfo>,
}

impl ::std::default::Default for Config {
  fn default() -> Self {
    Self {
      api_key: None,
      installed_uploads: HashMap::new(),
    }
  }
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
  #[arg(short, long, env = "SCRATCH_API_KEY")]
  /// Authenticate but don't save in the config
  api_key: Option<String>,

  #[arg(short, long, env = "SCRATCH_CONFIG_FILE")]
  /// The path where the config file is stored
  config_file: Option<PathBuf>,

  #[command(subcommand)]
  command: Commands,
}

#[derive(Subcommand)]
enum Commands {
  /// Save an API key to use in the other commands
  Auth {
    /// The API key to save
    api_key: String,
  },
  /// Retrieve information about the profile of the current user
  Profile,
  /// Retrieve information about a game given its ID
  Game {
    /// The ID of the game to retrieve information about
    game_id: u64,
  },
  /// List the collections of the profile, or the games of the collection
  Collections {
    /// If an ID is provided, list the games in its collection.
    collection_id: Option<u64>,
  },
  /// Download the upload with the given ID
  Download {
    /// The ID of the upload to download
    upload_id: u64,
    /// The path where the download folder will be placed
    /// 
    /// Defaults to ~/Games/{game_name}/
    #[arg(long)]
    install_path: Option<PathBuf>,
  },
}

fn load_config(custom_path: Option<&Path>) -> Config {
  let config_result = match custom_path {
    None => confy::load(APP_CONFIGURATION_NAME, APP_CONFIGURATION_FILE),
    Some(p) => confy::load_path(&p),
  };

  match config_result {
    Ok(c) => c,
    Err(e) => {
      eprintln_exit!("Error while reading configuration file!\n{}", e);
    }
  }
}

fn save_config(config: &Config, custom_path: Option<&Path>) {
  let config_result = match custom_path {
    None => confy::store(APP_CONFIGURATION_NAME, APP_CONFIGURATION_FILE, config),
    Some(p) => confy::store_path(&p, config),
  };

  if let Err(e) = config_result {
    eprintln_exit!("Error while saving to the configuration file!\n{}", e);
  }
}

// Returns the key's profile
async fn verify_key(client: &Client, api_key: &str, is_saved_key: bool) -> User {
  match scratch_io::get_profile(&client, &api_key).await {
    Ok(p) => p,
    Err(e) => {
      if !e.contains("invalid key") {
        eprintln_exit!("{e}");
      }
  
      if is_saved_key {
        eprintln_exit!("The key is not longer valid. Try logging in again.");
      } else {
        eprintln_exit!("The key is invalid!");
      }
    },
  }
}

// Retrieve information about a game_id and print it
// Also, print the available uploads of the game
async fn print_game_info(client: &Client, api_key: &str, game_id: u64) {
  match scratch_io::get_game_info(&client, &api_key, game_id).await {
    Ok(game_info) => println!("{game_info}"),
    Err(e) => eprintln_exit!("{e}"),
  };

  match scratch_io::get_game_uploads(&client, &api_key, game_id).await {
    Ok(uploads) => {
      let platforms = scratch_io::get_game_platforms(uploads.iter().collect());
      println!("  Platforms:");
      println!("{}", platforms.iter().map(|(uid, p)| format!("    {uid}, {}", p.to_string())).collect::<Vec<String>>().join("\n"));

      println!("  Uploads:");
      println!("{}", uploads.iter().map(|u| u.to_string()).collect::<Vec<String>>().join("\n"));
    }
    Err(e) => eprintln_exit!("{e}"),
  };
}

// Retrieve information about the profile's collections and print it
async fn print_collections(client: &Client, api_key: &str) {
  match scratch_io::get_collections(&client, &api_key).await {
    Ok(collections) => {
      for col in collections.iter() {
        println!("{col}");
      }
    }
    Err(e) => eprintln_exit!("{e}"),
  };
}

// Retrieve information about a collection's games and print it
async fn print_collection_games(client: &Client, api_key: &str, collection_id: u64) {
  match scratch_io::get_collection_games(&client, &api_key, collection_id).await {
    Ok(games) => {
      for cg in games {
        println!("{cg}");
      }
    }
    Err(e) => eprintln_exit!("{e}"),
  };
}

// Download a game's upload
async fn download(client: &Client, api_key: &str, upload_id: u64, dest: Option<&Path>) -> scratch_io::UploadInstallInfo {
  let progress_bar = indicatif::ProgressBar::hidden();
  progress_bar.set_style(
    indicatif::ProgressStyle::default_bar()
      .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})").unwrap()
      .progress_chars("#>-")
  );

  let download_response: Result<scratch_io::UploadInstallInfo, String> = scratch_io::download_upload(
    &client,
    &api_key,
    upload_id,
    dest,
    |u, g| {
      println!("\
Upload id: {}
  Game id: {}
  Game: {}
  Filename: {}",
        u.id,
        g.id,
        g.title,
        u.filename
      );
      progress_bar.set_length(u.size.unwrap_or(0));
    },
  |download_status| {
      match download_status {
        DownloadStatus::Warning(w) => println!("{w}"),
        DownloadStatus::DownloadedCover(c) => println!("Downloaded game cover to: {}", c.to_string_lossy()),
        DownloadStatus::StartingDownload() => {
          println!("Starting download...");
          progress_bar.set_draw_target(indicatif::ProgressDrawTarget::stderr());
        }
        DownloadStatus::Download(d) => progress_bar.set_position(d),
        DownloadStatus::Extract => println!("Extracting archive..."),
      };
    },
    std::time::Duration::from_millis(100)
  ).await;

  let upload_info =  match download_response {
    Ok(info) => info,
    Err(e) => eprintln_exit!("Error while downloading file:\n{}", e),
  }; 

  println!("Game upload downloaded to: {}", upload_info.upload_folder.to_string_lossy());

  upload_info
}

#[tokio::main]
async fn main() {
  
  // Read the user commands
  let cli: Cli = Cli::parse();

  // Get the config from the file
  let custom_config_file = cli.config_file.as_deref();
  let mut config: Config = load_config(custom_config_file);

  // Create reqwest client
  let client: Client = Client::new();
  
  /**** API KEY ****/

  // The api key is:
  // 1. If the command is auth, then the provided key
  // 2. If --api-key is set, then that key
  // 3. If not, then the saved config
  // 4. If there isn't a saved config, throw an error
  let api_key: String = if let Commands::Auth { ref api_key } = cli.command {
    api_key.clone()
  }
  else {
    cli.api_key.clone().unwrap_or(
      config.api_key.clone().unwrap_or_else(|| {
        eprintln_exit!("Error: an Itch.io API key is required, either via --api-key or the auth command.");
      })
    )
  };

  // Verify the key and get user info
  let profile: User = verify_key(
    &client,
    &api_key,
    // This is only true when the key is read from the config
    if let Commands::Auth { .. } = cli.command { false } else { cli.api_key.is_none() }
  ).await;

  /**** COMMANDS ****/

  match cli.command {
    Commands::Auth { api_key: _ } => {
      // We already checked if the key was valid
      println!("Valid key!");
      config.api_key = Some(api_key);
      
      // Save the valid key to the config file
      save_config(&config, custom_config_file);
      println!("The key was saved successfully.");

      // Print user info
      println!("Logged in as: {}", profile.username);
    }
    Commands::Profile => {
      println!("{profile}");
    }
    Commands::Game { game_id } => {
      print_game_info(&client, &api_key, game_id).await;
    }
    Commands::Collections { collection_id } => {
      match collection_id {
        None => print_collections(&client, &api_key).await,
        Some(id) => print_collection_games(&client, &api_key, id).await,
      }
    }
    Commands::Download { upload_id, install_path } => {
      if let Some(info) = config.installed_uploads.get(&upload_id) {
        eprintln_exit!("The game is already installed in: {}", info.upload_folder.to_string_lossy());
      }

      let upload_info = download(&client, &api_key, upload_id, install_path.as_deref()).await;
      config.installed_uploads.insert(upload_id, upload_info);

      save_config(&config, custom_config_file);
    }
  }
}
