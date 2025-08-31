use reqwest::Client;
use serde::{Serialize, Deserialize};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use scratch_io::{itch_api_types::*, serde_rules::*, DownloadStatus};

const APP_CONFIGURATION_NAME: &str = "scratch-io";
const APP_CONFIGURATION_FILE: &str = "config";

macro_rules! eprintln_exit {
  ($($arg:tt)*) => {{
    eprintln!($($arg)*);
    std::process::exit(1);
  }};
}

#[derive(Serialize, Deserialize)]
struct Config {
  api_key: Option<String>,
  #[serde(
    serialize_with = "serialize_u64_map",
    deserialize_with = "deserialize_u64_map"
  )]
  installed_uploads: HashMap<u64, scratch_io::InstalledUpload>,
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
  #[clap[flatten]]
  RequireApi(RequireApiCommands),
  #[clap[flatten]]
  OptionalApi(OptionalApiCommands),
}

// These commands will receive a valid API key and its profile
#[derive(Subcommand)]
enum RequireApiCommands {
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

// These commands may receive a valid API key, or may not
#[derive(Subcommand)]
enum OptionalApiCommands {
  /// List the installed games
  Installed,
  /// Remove a installed upload given its id
  Remove {
    /// The ID of the upload to remove
    upload_id: u64,
  }
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
async fn verify_key(client: &Client, api_key: &str, is_saved_key: bool) -> Result<User, String> {
  match scratch_io::get_profile(&client, &api_key).await {
    Ok(p) => Ok(p),
    Err(e) => {
      if !e.contains("invalid key") {
        return Err(e.to_string());
      }
  
      if is_saved_key {
        return Err(String::from("The key is not longer valid. Try logging in again."));
      } else {
        return Err(String::from("The key is invalid!"));
      }
    },
  }
}

async fn get_api_key(client: &Client, keys: &[Option<&str>], saved_key_index: usize) -> Result<(String, User), String> {
  let key_index = keys
    .iter()
    .position(|&k| k.is_some())
    .ok_or_else(|| String::from("Error: an Itch.io API key is required, either via --api-key or the auth command."))?;
  let api_key: String = keys[key_index].expect("If the index isn't valid, we should have exited before!").to_string();

  let is_saved_key = key_index == saved_key_index;
  
  // Verify the key and get user info
  let profile: User = verify_key(
    &client,
    api_key.as_str(),
    is_saved_key,
  ).await?;

  Ok((api_key, profile))
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
async fn download(client: &Client, api_key: &str, upload_id: u64, dest: Option<&Path>) -> scratch_io::InstalledUpload {
  let progress_bar = indicatif::ProgressBar::hidden();
  progress_bar.set_style(
    indicatif::ProgressStyle::default_bar()
      .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})").unwrap()
      .progress_chars("#>-")
  );

  let download_response: Result<scratch_io::InstalledUpload, String> = scratch_io::download_upload(
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

  match download_response {
    Ok(upload_info) => {
      println!("Game upload downloaded to: {}", upload_info.game_folder.join(upload_info.upload_id.to_string()).to_string_lossy());
      upload_info
    }
    Err(e) => eprintln_exit!("Error while downloading file:\n{}", e),
  }
}

// Retrieve information about a collection's games and print it
async fn print_installed_games(client: &Client, api_key: Option<&str>, installed_uploads: &mut HashMap<u64, scratch_io::InstalledUpload>) -> bool {
  let Some(key) = api_key else {
    for (_, iu) in installed_uploads {
      println!("{iu}");
    }
    println!("Warning: Couldn't update the game info!");
    return false
  };

  let mut updated = false;
  let mut print_warning = false;
  let mut last_error: String = String::new();

  for (_, iu) in installed_uploads {
    let res = iu.add_missing_info(&client, key, false).await;
    match res {
      Ok(u) => updated |= u,
      Err(e) => {
        print_warning = true;
        last_error = e;
      }
    }

    println!("{iu}");
  }

  if print_warning {
    println!("Warning: Couldn't update the game info!: {last_error}");
  }

  updated
}

async fn remove_upload(upload_id: u64, installed_uploads: &mut HashMap<u64, scratch_io::InstalledUpload>) {
  let upload_info = match installed_uploads.get(&upload_id) {
    None => eprintln_exit!("The given upload id is not installed!: {}", upload_id.to_string()),
    Some(f) => f,
  };
  
  if let Err(e) = scratch_io::remove(upload_id, &upload_info.game_folder).await {
    eprintln_exit!("Couldn't remove upload: {e}");
  }

  let upload_info = installed_uploads.remove(&upload_id)
    .expect("We have just checked if the key existed, and it did...");
  println!("Removed upload {upload_id} from: {}", &upload_info.game_folder.to_string_lossy())
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

  let api_key = get_api_key(
    &client,
    // The api key is:
    &[
      // 1. If the command is auth, then the provided key
      if let Commands::RequireApi(RequireApiCommands::Auth { api_key }) = &cli.command { Some(api_key.as_str()) } else { None },
      // 2. If --api-key is set, then that key
      cli.api_key.as_deref(),
      // 3. If not, then the saved config
      config.api_key.as_deref(),
      // 4. If there isn't a saved config, throw an error
    ],
    // The index of the previously saved config, to print a different error message
    2,
  ).await;

  /**** COMMANDS ****/

  match cli.command {
    Commands::RequireApi(command) => {
      let (api_key, profile) = api_key.unwrap_or_else(|e| eprintln_exit!("{e}"));

      match command {
        RequireApiCommands::Auth { api_key: _ } => {
          // We already checked if the key was valid
          println!("Valid key!");
          config.api_key = Some(api_key.clone());
          
          // Save the valid key to the config file
          save_config(&config, custom_config_file);
          println!("The key was saved successfully.");

          // Print user info
          println!("Logged in as: {}", api_key.as_str());
        }
        RequireApiCommands::Profile => {
          println!("{}", profile.to_string());
        }
        RequireApiCommands::Game { game_id } => {
          print_game_info(&client, api_key.as_str(), game_id).await;
        }
        RequireApiCommands::Collections { collection_id } => {
          match collection_id {
            None => print_collections(&client, api_key.as_str()).await,
            Some(id) => print_collection_games(&client, api_key.as_str(), id).await,
          }
        }
        RequireApiCommands::Download { upload_id, install_path } => {
          if let Some(info) = config.installed_uploads.get(&upload_id) {
            eprintln_exit!("The game is already installed in: {}", info.game_folder.join(info.upload_id.to_string()).to_string_lossy());
          }

          let upload_info = download(&client, api_key.as_str(), upload_id, install_path.as_deref()).await;
          config.installed_uploads.insert(upload_id, upload_info);

          save_config(&config, custom_config_file);
        }
      }
    }
    Commands::OptionalApi(command) => {
      let (api_key, _profile) = api_key.ok().unzip();

      match command {
        OptionalApiCommands::Installed => {
          let updated = print_installed_games(&client, api_key.as_deref(), &mut config.installed_uploads).await;

          if updated {
            save_config(&config, custom_config_file);
          }
        }
        OptionalApiCommands::Remove { upload_id } => {
          remove_upload(upload_id, &mut config.installed_uploads).await;
          save_config(&config, custom_config_file);
        }
      }
    }
  }
}
