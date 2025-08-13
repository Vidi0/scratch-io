use reqwest::Client;
use serde::{Serialize, Deserialize};
use clap::{Parser, Subcommand};
use scratch_io::itch_types::*;

const APP_CONFIGURATION_NAME: &str = "scratch-io";
const APP_CONFIGURATION_FILE: &str = "config";

#[derive(Serialize, Deserialize)]
#[derive(Debug)]
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
  #[arg(short, long)]
  /// Authenticate but don't save in the config
  api_key: Option<String>,

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
    upload_id: u64
  }
}

async fn print_game_info(client: &Client, api_key: &str, game_id: u64) {
  let game_info: Game = match scratch_io::get_game_info(&client, &api_key, game_id).await {
    Ok(info) => info,
    Err(e) => {
      eprintln!("Error while getting game info:\n{}", e);
      std::process::exit(1);
    },
  };

  let uploads: Vec<GameUpload> = match scratch_io::get_game_uploads(&client, &api_key, game_id).await {
    Ok(info) => info,
    Err(e) => {
      eprintln!("Error while getting game uploads:\n{}", e);
      std::process::exit(1);
    },
  };

  println!("{game_info}");
  println!("  Uploads:");
  for u in uploads.iter() {
    println!("{u}");
  }
}

async fn print_collections(client: &Client, api_key: &str) {
  let collections: Vec<Collection> = match scratch_io::get_collections(&client, &api_key).await {
    Ok(col) => col,
    Err(e) => {
      eprintln!("Error while getting collections:\n{}", e);
      std::process::exit(1);
    },
  };

  for col in collections.iter() {
    println!("{col}");
  }
}

async fn print_collection_games(client: &Client, api_key: &str, collection_id: u64) {
  let games: Vec<CollectionGame> = match scratch_io::get_collection_games(&client, &api_key, collection_id).await {
    Ok(g) => g,
    Err(e) => {
      eprintln!("Error while getting the collection's games:\n{}", e);
      std::process::exit(1);
    },
  };

  for cg in games {
  println!("{cg}");
  }
}

async fn download(client: &Client, api_key: &str, upload_id: u64) {
  let dest = std::path::Path::new("");
  let pb = indicatif::ProgressBar::new(0);
  pb.set_style(indicatif::ProgressStyle::default_bar()
    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})").unwrap()
    .progress_chars("#>-"));


  match scratch_io::download_upload(&client, &api_key, upload_id, dest, |file_size| {
    pb.set_length(file_size);
  }, |downloaded| {
    pb.set_position(downloaded);
  }).await {
    Err(e) => {
      pb.finish();
      eprintln!("Error while downloading file:\n{}", e);
      std::process::exit(1);
    }
    Ok(path) => {
      pb.finish();
      println!("File saved to: {}", path.to_string_lossy());
    }
  }

}

#[tokio::main]
async fn main() {

  // Get the config from the file
  let mut config: Config = match confy::load(APP_CONFIGURATION_NAME, APP_CONFIGURATION_FILE) {
    Ok(c) => c,
    Err(e) => {
      eprintln!("Error while reading configuration file!\n{}", e);
      std::process::exit(1);
    }
  };
  
  // Read the user commands
  let cli: Cli = Cli::parse();

  // Create reqwest client
  let client: Client = Client::new();
  
  // To authenticate, check if the key is valid and save it to the config
  if let Commands::Auth { api_key } = cli.command {
    // Check if the Itch.io API key is valid. If not, exit.
    if let Err(e) = scratch_io::verify_api_key(&client, &api_key).await {
      eprintln!("Error while validating key:\n{}", e);
      std::process::exit(1);
    }
    
    println!("Valid key!");
    config.api_key = Some(api_key);
    
    // Save the valid key to the config file
    if let Err(e) = confy::store(APP_CONFIGURATION_NAME, APP_CONFIGURATION_FILE, config) {
      eprintln!("Error while saving config:\n{}", e);
      std::process::exit(1);
    }
    
    // Exit
    println!("The key was saved successfully.");
    std::process::exit(0);
  }
  
  /**** API KEY ****/

  // The api key is:
  // 1. If --api-key is set, then that key
  // 2. If not, then the saved config
  // 3. If there isn't a saved config, throw an error
  let api_key: String = cli.api_key.unwrap_or(
    config.api_key.unwrap_or_else(|| {
      eprintln!("Error: an Itch.io API key is required, either via --api-key or the auth command.");
      std::process::exit(1);
    })
  );

  // Verify the key
  if let Err(e) = scratch_io::verify_api_key(&client, &api_key).await {
    eprintln!("Error while validating key:\n{}", e);
    if e.contains("invalid key") {
      eprintln!("Try logging in again.");
    }
    std::process::exit(1);
  }

  /**** COMMANDS ****/

  match cli.command {
    Commands::Auth { api_key: _ } => {
      panic!("Already checked if the command is Auth! The other check should have exited. This should NEVER happen!");
    },
    Commands::Game { game_id } => {
      print_game_info(&client, &api_key, game_id).await;
    },
    Commands::Collections { collection_id } => {
      match collection_id {
        None => print_collections(&client, &api_key).await,
        Some(id) => print_collection_games(&client, &api_key, id).await,
      }
    },
    Commands::Download { upload_id } => {
      download(&client, &api_key, upload_id).await;
    }
  }
}
