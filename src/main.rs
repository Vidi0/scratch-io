use reqwest::Client;
use serde::{Serialize, Deserialize};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use scratch_io::itch_types::*;

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
}

impl ::std::default::Default for Config {
  fn default() -> Self {
    Self {
      api_key: None,
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
  config_file: Option<String>,

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
    path: Option<PathBuf>,
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
      println!("  Uploads:");
      for u in uploads.iter() {
        println!("{u}");
      }
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
async fn download(client: &Client, api_key: &str, upload_id: u64, dest: Option<&Path>) {
  let progress_bar = indicatif::ProgressBar::new(0);
  progress_bar.set_style(
    indicatif::ProgressStyle::default_bar()
      .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})").unwrap()
      .progress_chars("#>-")
  );

  let download_response: Result<(PathBuf, String), String> = scratch_io::download_upload(
    &client,
    &api_key,
    upload_id,
    dest,
    |upload, game| {
      println!("\
Upload id: {}
  Game id: {}
  Game: {}
  Filename: {}",
        upload.id,
        game.id,
        game.title,
        upload.filename
      );
      progress_bar.set_length(upload.size.unwrap_or(0));
    },
  |downloaded| {
      progress_bar.set_position(downloaded);
    },
    std::time::Duration::from_millis(100)
  ).await;

  match download_response {
    Ok((path, log)) => {
      progress_bar.finish();
      print!("{log}");

      if path.is_file() {
        println!("Game file saved to: {}", path.to_string_lossy());
      } else {
        println!("Game folder extracted to: {}", path.to_string_lossy());
      }
    }
    Err(e) => {
      progress_bar.abandon();
      eprintln_exit!("Error while downloading file:\n{}", e);
    }
  }
}

#[tokio::main]
async fn main() {
  
  // Read the user commands
  let cli: Cli = Cli::parse();

  // Get the config from the file
  let mut config: Config = {
    let config_result = match cli.config_file {
      None => confy::load(APP_CONFIGURATION_NAME, APP_CONFIGURATION_FILE),
      Some(f) => confy::load_path(&f),
    };

    match config_result {
      Ok(c) => c,
      Err(e) => {
        eprintln_exit!("Error while reading configuration file!\n{}", e);
      }
    }
  };

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
      config.api_key.unwrap_or_else(|| {
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
      if let Err(e) = confy::store(APP_CONFIGURATION_NAME, APP_CONFIGURATION_FILE, config) {
        eprintln_exit!("Error while saving config:\n{}", e);
      }
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
    Commands::Download { upload_id, path } => {
      download(&client, &api_key, upload_id, path.as_deref()).await;
    }
  }
}
