use reqwest::Client;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use scratch_io::{itch_api_types::*, DownloadStatus, InstalledUpload};

mod config;
use config::Config;

#[macro_export]
macro_rules! eprintln_exit {
  ($($arg:tt)*) => {{
    eprintln!($($arg)*);
    std::process::exit(1);
  }};
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
  /// Log in with an API key to use in the other commands
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
  /// List the profile's collections
  Collections,
  /// List the games in the given collection
  CollectionGames {
    /// The ID of the collection where the games are located.
    collection_id: u64,
  },
  /// Download a game cover gives its game ID
  DownloadCover {
    /// The ID of the game from which the cover will be downloaded
    game_id: u64,
    /// The filename of the downloaded cover image (without extension)
    #[arg(long, env = "SCRATCH_FILENAME")]
    filename: Option<String>,
    /// The path where the downloaded file will be placed
    /// 
    /// Defaults to ~/Games/{game_name}/
    #[arg(long, env = "SCRATCH_FOLDER")]
    folder: Option<PathBuf>,
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
  /// Login with a username and password
  Login {
    /// The username of the user who logs in
    #[arg(env = "SCRATCH_USERNAME")]
    username: String,
    /// The password of the user who logs in
    #[arg(env = "SCRATCH_PASSWORD")]
    password: String,
    /// The response of the reCAPTCHA (if required)
    #[arg(long, env = "SCRATCH_RECAPTCHA_RESPONSE")]
    recaptcha_response: Option<String>,
    /// The TOTP 2nd factor authentication
    #[arg(long, env = "SCRATCH_TOTP_CODE")]
    totp_code: Option<u64>,
  },
  /// Remove the saved API key
  Logout,
  /// List the installed games
  Installed,
  /// Get the installed information about an upload given its ID
  InstalledUpload {
    /// The ID of the upload to retrieve information about
    upload_id: u64,
  },
  /// Remove a installed upload given its ID
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
  #[command(group(clap::ArgGroup::new("launch_method").required(true).multiple(true)))]
  Launch {
    /// The ID of the upload to launch
    upload_id: u64,
    /// The itch manifest's action to call the game with
    /// 
    /// Returns an error if the action isn't present in the manifest, or the manifest is missing
    #[arg(long, env = "SCRATCH_LAUNCH_ACTION", group = "launch_method")]
    launch_action: Option<String>,
    /// The platform for which the game binary will be searched
    /// 
    /// The Itch.io uploads don't specify a game binary, so which file to run will be decided by heuristics.
    /// 
    /// The heuristics need to know which platform is the executable they are searching.
    #[arg(long, env = "SCRATCH_PLATFORM", group = "launch_method")]
    platform: Option<scratch_io::GamePlatform>,
    /// Instead of the platform (or in addition to), a executable path can be provided
    #[arg(long, env = "SCRATCH_UPLOAD_EXECUTABLE_PATH", group = "launch_method")]
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

async fn get_api_key(client: &Client, keys: &[Option<&str>], saved_key_index: usize) -> Result<(String, User), String> {
  let key_index = keys
    .iter()
    .position(|&k| k.is_some())
    .ok_or_else(|| String::from("Error: an Itch.io API key is required, either via --api-key, auth, or the login command."))?;
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

fn exit_if_already_installed(upload_id: u64, installed_uploads: &HashMap<u64, InstalledUpload>) {
  if let Some(info) = installed_uploads.get(&upload_id) {
    eprintln_exit!("The game is already installed in: \"{}\"", info.game_folder.join(info.upload_id.to_string()).to_string_lossy());
  }
}

// Save a key to the config and print info
fn auth(key: String, config_api_key: &mut Option<String>, profile: User) {
  // We already checked if the key was valid
  println!("Valid key!");
  *config_api_key = Some(key);
          
  // Print user info
  println!("Logged in as: {}", profile.get_name());
}

// Login with an username and password, save to the config and print info
async fn login(client: &Client, username: &str, password: &str, recaptcha_response: Option<&str>, totp_code: Option<u64>, config_api_key: &mut Option<String>) {
  let ls = scratch_io::login(client, username, password, recaptcha_response, totp_code).await.unwrap_or_else(|e| eprintln_exit!("{e}"));
  let profile = scratch_io::get_profile(client, ls.key.key.as_str()).await.unwrap_or_else(|e| eprintln_exit!("{e}"));

  auth(ls.key.key, config_api_key, profile);
}

// Remove the saved API key (if any)
fn logout(config_api_key: &mut Option<String>) {
  match config_api_key {
    None => eprintln!("There isn't any API key saved!"),
    Some(_) => {
      *config_api_key = None;
      println!("Logged out.");
    }
  }
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
  let game_info = scratch_io::get_game_info(&client, &api_key, game_id).await.unwrap_or_else(|e| eprintln_exit!("{e}"));
  println!("{game_info}");

  let uploads = scratch_io::get_game_uploads(&client, &api_key, game_id).await.unwrap_or_else(|e| eprintln_exit!("{e}"));
  let platforms = scratch_io::get_game_platforms(uploads.as_slice());
  
  println!("  Platforms:");
  println!("{}", platforms.iter().map(|(uid, p)| format!("    {uid}, {p}")).collect::<Vec<String>>().join("\n"));
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
async fn download(client: &Client, api_key: &str, upload_id: u64, dest: Option<&Path>, installed_uploads: &mut HashMap<u64, InstalledUpload>) {
  exit_if_already_installed(upload_id, installed_uploads);

  let progress_bar = indicatif::ProgressBar::hidden();
  progress_bar.set_style(
    indicatif::ProgressStyle::default_bar()
      .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})").unwrap()
      .progress_chars("#>-")
  );

  let iu = scratch_io::download_upload(
    &client,
    &api_key,
    upload_id,
    dest,
    |u, g| println!("\
Upload id: {}
  Game id: {}
  Game: {}
  Filename: {}",
      u.id,
      g.id,
      g.title,
      u.filename
    ),
    |download_status| {
      match download_status {
        DownloadStatus::Warning(w) => println!("{w}"),
        DownloadStatus::DownloadedCover { game_cover_path } => println!("Downloaded game cover to: \"{}\"", game_cover_path.to_string_lossy()),
        DownloadStatus::StartingDownload { bytes_to_download } => {
          println!("Starting download...");
          progress_bar.set_length(bytes_to_download);
          progress_bar.set_draw_target(indicatif::ProgressDrawTarget::stderr());
        }
        DownloadStatus::DownloadProgress { downloaded_bytes } => progress_bar.set_position(downloaded_bytes),
        DownloadStatus::Extract => println!("Extracting archive..."),
      };
    },
    std::time::Duration::from_millis(100)
  ).await.inspect(|iu| println!("Game upload downloaded to: \"{}\"", iu.game_folder.join(iu.upload_id.to_string()).to_string_lossy()))
    .unwrap_or_else(|e| eprintln_exit!("Error while downloading file!\n{}", e));

  installed_uploads.insert(upload_id, iu);
}

async fn download_cover(client: &Client, api_key: &str, game_id: u64, filename: Option<&str>, folder: Option<&Path>) {
  scratch_io::download_game_cover_from_id(client, api_key, game_id, filename, folder).await.unwrap_or_else(|e| eprintln_exit!("{e}"));
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

// Print the installed info of an upload
async fn print_installed_upload(client: &Client, api_key: Option<&str>, upload_id: u64, installed_uploads: &mut HashMap<u64, InstalledUpload>) -> bool {
  
  let iu = get_installed_upload_info_mut(upload_id, installed_uploads);
  let mut updated = false;

  if let Some(key) = api_key {
    match iu.add_missing_info(&client, key, false).await {
      Ok(u) => updated |= u,
      Err(e) => println!("Warning: Couldn't update the game info!: {e}"),
    }
  } else {
    println!("Warning: Couldn't update the game info!: Missing, invalid or couldn't verify the api key.")
  }

  println!("{iu}");

  let manifest = scratch_io::get_upload_manifest(upload_id, &iu.game_folder).await
    .unwrap_or_else(|e| eprintln_exit!("Couldn't get the itch manifest of the upload!: {e}"));

  if let Some(m) = manifest {
    println!("{m}");
  }

  updated
}

// Import an already installed upload from a folder
async fn import(client: &Client, api_key: &str, upload_id: u64, game_folder: &Path, installed_uploads: &mut HashMap<u64, InstalledUpload>) {
  exit_if_already_installed(upload_id, installed_uploads);

  let iu = scratch_io::import(client, api_key, upload_id, game_folder).await
    .inspect(|ui| println!("Game imported from: \"{}\"", ui.game_folder.join(ui.upload_id.to_string()).to_string_lossy()))
    .unwrap_or_else(|e| eprintln_exit!("Error while importing game!\n{}", e));

  installed_uploads.insert(upload_id, iu);
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

  println!("Moved upload {upload_id}\n  Source: \"{}\"\n  Destination: \"{}\"", src_game_folder.to_string_lossy(), upload_info.game_folder.to_string_lossy());
}

// Launch an installed upload
async fn launch_upload(
  upload_id: u64,
  upload_executable_path: Option<&Path>,
  launch_action: Option<&str>,
  platform: Option<&scratch_io::GamePlatform>,
  wrapper: Option<&str>,
  game_arguments: Option<&str>,
  installed_uploads: &HashMap<u64, InstalledUpload>
) {
  let upload_info = get_installed_upload_info(upload_id, installed_uploads);
  let game_folder = upload_info.game_folder.to_path_buf();
  
  let wrapper: Vec<String> = wrapper.map_or(Vec::new(), |w|
    shell_words::split(w).unwrap_or_else(|e| eprintln_exit!("Couldn't split the wrapper arguments: {w}\n{e}"))
  );
  
  let game_arguments: Vec<String> = game_arguments.map_or(Vec::new(), |a|
    shell_words::split(a).unwrap_or_else(|e| eprintln_exit!("Couldn't split the game arguments: {a}\n{e}"))
  );

  let launch_method = if let Some(p) = upload_executable_path {
    scratch_io::LaunchMethod::AlternativeExecutable(p)
  } else if let Some(la) = launch_action {
    scratch_io::LaunchMethod::ManifestAction(la)
  } else if let Some(p) = platform {
    scratch_io::LaunchMethod::Heuristics(p, upload_info.game.as_ref().unwrap_or_else(|| eprintln_exit!(r#"Missing game or upload info. Use the "installed" command to fill missing info"#)))
  } else {
    eprintln_exit!(r#"A launch method is required! One of: "launch_action", "platform" or "upload_executable_path" must exist!"#)
  };

  scratch_io::launch(
    upload_id,
    game_folder.as_path(),
    launch_method,
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
  let custom_config_file = cli.config_file;
  let mut config: Config = Config::load_unwrap(custom_config_file.clone()).await;

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
        RequireApiCommands::Auth { api_key: k } => {
          auth(k, &mut config.api_key, profile);
          config.save_unwrap(custom_config_file).await;
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
        RequireApiCommands::Collections => {
          print_collections(&client, api_key.as_str()).await;
        }
        RequireApiCommands::CollectionGames { collection_id } => {
          print_collection_games(&client, api_key.as_str(), collection_id).await;
        }
        RequireApiCommands::Download { upload_id, install_path } => {
          download(&client, api_key.as_str(), upload_id, install_path.as_deref(), &mut config.installed_uploads).await;
          config.save_unwrap(custom_config_file).await;
        }
        RequireApiCommands::DownloadCover { game_id, filename, folder } => {
          download_cover(&client, api_key.as_str(), game_id, filename.as_deref(), folder.as_deref()).await;
        }
        RequireApiCommands::Import { upload_id, install_path } => {
          import(&client, api_key.as_str(), upload_id, install_path.as_path(), &mut config.installed_uploads).await;
          config.save_unwrap(custom_config_file).await;
        }
      }
    }
    Commands::OptionalApi(command) => {
      let (api_key, _profile) = api_key.ok().unzip();

      match command {
        OptionalApiCommands::Login { username, password, recaptcha_response, totp_code } => {
          login(&client, username.as_str(), password.as_str(), recaptcha_response.as_deref(), totp_code, &mut config.api_key).await;
          config.save_unwrap(custom_config_file).await;
        }
        OptionalApiCommands::Logout => {
          logout(&mut config.api_key);
          config.save_unwrap(custom_config_file).await;
        }
        OptionalApiCommands::Installed => {
          if print_installed_games(&client, api_key.as_deref(), &mut config.installed_uploads).await {
            config.save_unwrap(custom_config_file).await;
          }
        }
        OptionalApiCommands::InstalledUpload { upload_id } => {
          if print_installed_upload(&client, api_key.as_deref(), upload_id, &mut config.installed_uploads).await {
            config.save_unwrap(custom_config_file).await;
          }
        }
        OptionalApiCommands::Remove { upload_id } => {
          remove_upload(upload_id, &mut config.installed_uploads).await;
          config.save_unwrap(custom_config_file).await;
        }
        OptionalApiCommands::Move { upload_id, game_path_dst } => {
          move_upload(upload_id, game_path_dst.as_path(), &mut config.installed_uploads).await;
          config.save_unwrap(custom_config_file).await;
        }
        OptionalApiCommands::Launch { upload_id, launch_action, platform, upload_executable_path, wrapper, game_arguments } => {
          launch_upload(upload_id, upload_executable_path.as_deref(), launch_action.as_deref(), platform.as_ref(), wrapper.as_deref(), game_arguments.as_deref(), &config.installed_uploads).await;
        }
      }
    }
  }
}
