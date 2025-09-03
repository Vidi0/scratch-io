use reqwest::Client;
use serde::{Serialize, Deserialize};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use scratch_io::{itch_api_types::*, serde_rules::*, DownloadStatus, InstalledUpload};

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
  installed_uploads: HashMap<u64, InstalledUpload>,
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
  /// Authenticate but don't save in the config
  #[arg(short, long, env = "SCRATCH_API_KEY")]
  api_key: Option<String>,

  /// The path where the config file is stored
  #[arg(short, long, env = "SCRATCH_CONFIG_FILE")]
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
  /// List the game keys owned by the user
  Owned,
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
    #[arg(long, env = "SCRATCH_INSTALL_PATH")]
    install_path: Option<PathBuf>,
  },
  /// Imports an already installed game given its upload ID and the game folder
  Import {
    /// The ID of the upload to import
    upload_id: u64,
    /// The path where the game folder is located
    install_path: PathBuf,
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
  },
  /// Move a installed upload to another game folder
  Move {
    /// The ID of the upload to import
    upload_id: u64,
    /// The path where the game folder will be placed
    game_path_dst: PathBuf,
  },
  /// Launchs an installed game given its upload ID and the platform or executable path
  #[command(group(clap::ArgGroup::new("upload_target").required(true).multiple(true)))]
  Launch {
    /// The ID of the upload to launch
    upload_id: u64,
    /// The platform for which the game binary will be searched
    /// 
    /// The Itch.io uploads don't specify a game binary, so which file to run will be decided by heuristics.
    /// 
    /// The heuristics need to know which platform is the executable they are searching.
    #[arg(value_enum, group = "upload_target")]
    platform: Option<scratch_io::GamePlatform>,
    /// Instead of the platform (or in addition to), a executable path can be provided
    #[arg(long, env = "SCRATCH_UPLOAD_EXECUTABLE_PATH", group = "upload_target")]
    upload_executable_path: Option<PathBuf>,
    /// A wrapper command to launch the game with
    #[arg(long, env = "SCRATCH_WRAPPER")]
    wrapper: Option<String>,
    /// The arguments the game will be called with
    /// 
    /// There arguments will be split into a vector according to parsing rules of UNIX shell
    #[arg(long, env = "SCRATCH_GAME_ARGUMENTS")]
    game_arguments: Option<String>,
  },
}

fn load_config(custom_path: Option<&Path>) -> Config {
  match custom_path {
    None => confy::load(APP_CONFIGURATION_NAME, APP_CONFIGURATION_FILE),
    Some(p) => confy::load_path(&p),
  }.unwrap_or_else(|e| eprintln_exit!("Error while reading configuration file!\n{}", e))
}

fn save_config(config: &Config, custom_path: Option<&Path>) {
  match custom_path {
    None => confy::store(APP_CONFIGURATION_NAME, APP_CONFIGURATION_FILE, config),
    Some(p) => confy::store_path(&p, config),
  }.unwrap_or_else(|e| eprintln_exit!("Error while saving to the configuration file!\n{}", e))
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

fn get_installed_upload_info(upload_id: u64, installed_uploads: &HashMap<u64, InstalledUpload>) -> &InstalledUpload {
  installed_uploads.get(&upload_id).unwrap_or_else(|| eprintln_exit!("The given upload id is not installed!: {}", upload_id.to_string()))
}

fn get_installed_upload_info_mut(upload_id: u64, installed_uploads: &mut HashMap<u64, InstalledUpload>) -> &mut InstalledUpload {
  installed_uploads.get_mut(&upload_id).unwrap_or_else(|| eprintln_exit!("The given upload id is not installed!: {}", upload_id.to_string()))
}

// Return the user profile
async fn verify_key(client: &Client, api_key: &str, is_saved_key: bool) -> Result<User, String> {
  scratch_io::get_profile(&client, &api_key).await.map_err(|e| {
    if !e.contains("invalid key") {
      e
    } else if is_saved_key {
      format!("The key is not longer valid. Try logging in again.")
    } else {
      format!("The key is invalid!")
    }
  })
}

// List the owned game keys
async fn print_owned_keys(client: &Client, api_key: &str) {
  let keys = scratch_io::get_owned_keys(&client, &api_key).await.unwrap_or_else(|e| eprintln_exit!("{e}"));

  println!("{}", keys.iter().map(|k| k.to_string()).collect::<Vec<String>>().join("\n"));
}


// Print information about a game, including its uploads and platforms
async fn print_game_info(client: &Client, api_key: &str, game_id: u64) {
  println!("{}", scratch_io::get_game_info(&client, &api_key, game_id).await.unwrap_or_else(|e| eprintln_exit!("{e}")));

  let uploads = scratch_io::get_game_uploads(&client, &api_key, game_id).await.unwrap_or_else(|e| eprintln_exit!("{e}"));
  let platforms = scratch_io::get_game_platforms(uploads.as_slice());

  println!("  Platforms:");
  println!("{}", platforms.iter().map(|(uid, p)| format!("    {uid}, {}", p.to_string())).collect::<Vec<String>>().join("\n"));
  println!("  Uploads:");
  println!("{}", uploads.iter().map(|u| u.to_string()).collect::<Vec<String>>().join("\n"));
}

// Print information about the user's collections
async fn print_collections(client: &Client, api_key: &str) {
  for col in scratch_io::get_collections(&client, &api_key).await.unwrap_or_else(|e| eprintln_exit!("{e}")) {
    println!("{col}");
  }
}

// Print the games listed in a collection
async fn print_collection_games(client: &Client, api_key: &str, collection_id: u64) {
  for cg in scratch_io::get_collection_games(&client, &api_key, collection_id).await.unwrap_or_else(|e| eprintln_exit!("{e}")) {
    println!("{cg}");
  }
}

// Download a game's upload
async fn download(client: &Client, api_key: &str, upload_id: u64, dest: Option<&Path>) -> InstalledUpload {
  let progress_bar = indicatif::ProgressBar::hidden();
  progress_bar.set_style(
    indicatif::ProgressStyle::default_bar()
      .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})").unwrap()
      .progress_chars("#>-")
  );

  scratch_io::download_upload(
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
        DownloadStatus::DownloadedCover(c) => println!("Downloaded game cover to: \"{}\"", c.to_string_lossy()),
        DownloadStatus::StartingDownload() => {
          println!("Starting download...");
          progress_bar.set_draw_target(indicatif::ProgressDrawTarget::stderr());
        }
        DownloadStatus::Download(d) => progress_bar.set_position(d),
        DownloadStatus::Extract => println!("Extracting archive..."),
      };
    },
    std::time::Duration::from_millis(100)
  ).await.inspect(|iu| println!("Game upload downloaded to: \"{}\"", iu.game_folder.join(iu.upload_id.to_string()).to_string_lossy()))
    .unwrap_or_else(|e| eprintln_exit!("Error while downloading file!\n{}", e))
}

// Print a list of the currently installed games
async fn print_installed_games(client: &Client, api_key: Option<&str>, installed_uploads: &mut HashMap<u64, InstalledUpload>) -> bool {
  let mut updated = false;
  let mut warning: (bool, String) = (false, String::new());

  for (_, iu) in installed_uploads {
    if let Some(key) = api_key {
      match iu.add_missing_info(&client, key, false).await {
        Ok(u) => updated |= u,
        Err(e) => warning = (true, e.to_string())
      }
    } else {
      warning = (true, format!("Missing, invalid or couldn't verify the api key."))
    }

    println!("{iu}");
  }

  if warning.0 {
    println!("Warning: Couldn't update the game info!: {}", warning.1);
  }

  updated
}

// Import an already installed upload from a folder
async fn import(client: &Client, api_key: &str, upload_id: u64, game_folder: &Path) -> InstalledUpload {
  scratch_io::import(client, api_key, upload_id, game_folder).await
    .inspect(|ui| println!("Game imported from: \"{}\"", ui.game_folder.join(ui.upload_id.to_string()).to_string_lossy()))
    .unwrap_or_else(|e| eprintln_exit!("Error while importing game!\n{}", e))
}

// Remove an installed upload from the system
async fn remove_upload(upload_id: u64, installed_uploads: &mut HashMap<u64, InstalledUpload>) {
  let upload_info = get_installed_upload_info(upload_id, installed_uploads);
  
  scratch_io::remove(upload_id, &upload_info.game_folder).await
    .unwrap_or_else(|e| eprintln_exit!("Couldn't remove upload!\n{e}"));

  println!("Removed upload {upload_id} from: \"{}\"", &upload_info.game_folder.to_string_lossy());
  
  installed_uploads.remove(&upload_id)
    .expect("We have just checked if the key existed, and it did...");
}

// Move an installed upload from a place to another
async fn move_upload(upload_id: u64, dst_game_folder: &Path, installed_uploads: &mut HashMap<u64, InstalledUpload>) {
  let upload_info = get_installed_upload_info_mut(upload_id, installed_uploads);

  let src_game_folder = upload_info.game_folder.to_path_buf();

  upload_info.game_folder = scratch_io::r#move(upload_id, src_game_folder.as_path(), dst_game_folder).await
    .unwrap_or_else(|e| eprintln_exit!("Couldn't move upload!\n{e}"));

  println!("Moved upload {upload_id}\n  from: \"{}\"\n  to: \"{}\"", src_game_folder.to_string_lossy(), upload_info.game_folder.to_string_lossy());
}

// Launch an installed upload
async fn launch_upload(
  upload_id: u64,
  platform: Option<scratch_io::GamePlatform>,
  upload_executable_path: Option<&Path>,
  wrapper: Option<String>,
  game_arguments: Option<String>,
  installed_uploads: &HashMap<u64, InstalledUpload>
) {
  let upload_info = get_installed_upload_info(upload_id, installed_uploads);
  let game_folder = upload_info.game_folder.to_path_buf();

  let heuristics_info: Option<(&scratch_io::GamePlatform, &Game)> = match platform {
    None => None,
    Some(ref p) => Some((p, upload_info.game.as_ref().unwrap_or_else(|| eprintln_exit!("Missing game or upload info. Use the \"installed\" command to fill missing info")))),
  };

  let game_arguments: Vec<String>= game_arguments.map_or(Vec::new(), |a|
    shell_words::split(a.as_str()).unwrap_or_else(|e| eprintln_exit!("Couldn't split the game arguments: {a}\n{e}"))
  );

  let wrapper: Vec<String>= wrapper.map_or(Vec::new(), |w: String|
    shell_words::split(w.as_str()).unwrap_or_else(|e| eprintln_exit!("Couldn't split the wrapper arguments: {w}\n{e}"))
  );

  scratch_io::launch(
    upload_id,
    game_folder.as_path(),
    heuristics_info,
    upload_executable_path,
    wrapper.as_slice(),
    game_arguments.as_slice(),
    |up, command| println!("Launching game:\n  Executable path: \"{}\"\n  Command: {command}", up.to_string_lossy())
  ).await
    .unwrap_or_else(|e| eprintln_exit!("Couldn't launch: {upload_id}\n{e}"));
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
          println!("Logged in as: {}", profile.get_name());
        }
        RequireApiCommands::Profile => {
          println!("{}", profile.to_string());
        }
        RequireApiCommands::Owned => {
          print_owned_keys(&client, api_key.as_str()).await;
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
            eprintln_exit!("The game is already installed in: \"{}\"", info.game_folder.join(info.upload_id.to_string()).to_string_lossy());
          }

          let upload_info = download(&client, api_key.as_str(), upload_id, install_path.as_deref()).await;
          config.installed_uploads.insert(upload_id, upload_info);

          save_config(&config, custom_config_file);
        }
        RequireApiCommands::Import { upload_id, install_path } => {
          if let Some(info) = config.installed_uploads.get(&upload_id) {
            eprintln_exit!("The game is already imported and placed in: \"{}\"", info.game_folder.join(info.upload_id.to_string()).to_string_lossy());
          }

          let upload_info = import(&client, api_key.as_str(), upload_id, install_path.as_path()).await;
          config.installed_uploads.insert(upload_id, upload_info);

          save_config(&config, custom_config_file);
        }
      }
    }
    Commands::OptionalApi(command) => {
      let (api_key, _profile) = api_key.ok().unzip();

      match command {
        OptionalApiCommands::Installed => {
          if print_installed_games(&client, api_key.as_deref(), &mut config.installed_uploads).await {
            save_config(&config, custom_config_file);
          }
        }
        OptionalApiCommands::Remove { upload_id } => {
          remove_upload(upload_id, &mut config.installed_uploads).await;
          save_config(&config, custom_config_file);
        }
        OptionalApiCommands::Move { upload_id, game_path_dst } => {
          move_upload(upload_id, game_path_dst.as_path(), &mut config.installed_uploads).await;
          save_config(&config, custom_config_file);
        }
        OptionalApiCommands::Launch { upload_id, platform, upload_executable_path, wrapper, game_arguments } => {
          launch_upload(upload_id, platform, upload_executable_path.as_deref(), wrapper, game_arguments, &config.installed_uploads).await;
        }
      }
    }
  }
}
