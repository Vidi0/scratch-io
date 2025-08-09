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
  let config: Result<Config, confy::ConfyError> = confy::load(APP_CONFIGURATION_NAME, APP_CONFIGURATION_FILE);
  if let Err(err) = config {
    eprintln!("Error while reading configuration file!\n{}", err);
    std::process::exit(1);
  }
  let mut config: Config = config.ok().unwrap();
  
  // Read the user commands
  let cli: Cli = Cli::parse();
  
  // To authenticate, check if the key is valid and save it to the config
  if let Commands::Auth { api_key } = cli.command {
    // Check if the Itch.io API key is valid. If not, exit.
    let is_valid: Result<(), String> = scratch_io::verify_api_key(&api_key).await;
    if is_valid.is_err() {
      eprintln!("Error while validating key:\n{}", is_valid.err().unwrap());
      std::process::exit(1);
    }
    
    println!("Valid key!");
    config.api_key = Some(api_key);
    
    // Save the valid key to the config file
    let save_result: Result<(), confy::ConfyError> = confy::store(APP_CONFIGURATION_NAME, APP_CONFIGURATION_FILE, config);
    if save_result.is_err() {
      eprintln!("Error while saving config:\n{}", save_result.err().unwrap());
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
  let api_key: String = if cli.api_key.is_none() {
    if config.api_key.is_none() {
      eprintln!("Error: an Itch.io API key is required, either via --api-key or the auth command.");
      std::process::exit(1);
    }
    else {
      config.api_key.unwrap()
    }
  }
  else {
    cli.api_key.unwrap()
  };

  // Verify the key
  {
    let is_valid: Result<(), String> = scratch_io::verify_api_key(&api_key).await;
    if is_valid.is_err() {
      eprintln!("Error while validating key:\n{}", is_valid.err().unwrap());
      std::process::exit(1);
    }
  }

  /**** COMMANDS ****/

  match cli.command {
    Commands::Auth { api_key: _ } => {
      panic!("Already checked if the command is Auth! The other check should have exited. This should NEVER happen!");
    }
    Commands::Game { id } => {
      let game_info: Result<scratch_io::itch_types::Game, String> = scratch_io::get_game_info(api_key, id).await;
      if game_info.is_err() {
        eprintln!("Error while getting game info:\n{}", game_info.err().unwrap());
        std::process::exit(1);
      }
      let game_info: scratch_io::itch_types::Game = game_info.ok().unwrap();

      println!("Id: {}
Game: {}
  Description:  {}
  URL:  {}
  Classification: {}
  Type: {}
  Cover URL:  {}
  Published at: {}
  Created at:   {}",
      game_info.id,
      game_info.title,
      game_info.short_text.unwrap_or(String::from("None")),
      game_info.url,
      game_info.classification.unwrap_or(String::from("None")),
      game_info.r#type.unwrap_or(String::from("None")),
      game_info.cover_url.unwrap_or(String::from("None")),
      game_info.published_at.unwrap_or(String::from("None")),
      game_info.created_at.unwrap_or(String::from("None")));
    }
  }
}
