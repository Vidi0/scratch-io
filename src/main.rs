use serde::{Serialize, Deserialize};
use clap::{Parser, Subcommand};

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
    api_key: String,
  },
  /// Retrieve information about a game given its ID
  Game {
    id: u64,
  },
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
  
  // To authenticate, check if the key is valid and save it to the config
  if let Commands::Auth { api_key } = cli.command {
    // Check if the Itch.io API key is valid. If not, exit.
    if let Err(e) = scratch_io::verify_api_key(&api_key).await {
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
  if let Err(e) = scratch_io::verify_api_key(&api_key).await {
    eprintln!("Error while validating key:\n{}", e);
    std::process::exit(1);
  }

  /**** COMMANDS ****/

  match cli.command {
    Commands::Auth { api_key: _ } => {
      panic!("Already checked if the command is Auth! The other check should have exited. This should NEVER happen!");
    },
    Commands::Game { id } => {
      let game_info: scratch_io::itch_types::Game = match scratch_io::get_game_info(&api_key, id).await {
        Ok(info) => info,
        Err(e) => {
          eprintln!("Error while getting game info:\n{}", e);
          std::process::exit(1);
        },
      };

      println!("\
Id: {}
Game: {}
  Description:  {}
  URL:  {}
  Cover URL:  {}
  Price:  {}
  Classification: {}
  Type: {}
  Published at: {}
  Created at: {}",
        game_info.id,
        game_info.title,
        game_info.short_text.unwrap_or(String::new()),
        game_info.url,
        game_info.cover_url.unwrap_or(String::new()),
        match game_info.min_price {
          None => String::new(),
          Some(p) => {
            if p <= 0 { String::from("Free") } else { String::from("Paid") }
          }
        },
        game_info.classification,
        game_info.r#type,
        game_info.published_at.unwrap_or(String::new()),
        game_info.created_at.unwrap_or(String::new())
      );
    }
  }
}
