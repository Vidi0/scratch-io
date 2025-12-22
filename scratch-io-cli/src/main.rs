mod config;

use config::Config;

use clap::{Parser, Subcommand};
use scratch_io::itch_api::{
  self, ItchClient,
  types::{BuildID, CollectionID, GameID, UploadID, UserID},
};
use scratch_io::{DownloadStatus, InstalledUpload};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

// This enum is a copy of scratch_io::GamePlatform that derives clap::ValueEnum
#[derive(clap::ValueEnum, Clone)]
enum GamePlatform {
  Linux,
  Windows,
  Osx,
  Android,
  Web,
  Flash,
  Java,
  UnityWebPlayer,
}

impl From<GamePlatform> for scratch_io::GamePlatform {
  fn from(value: GamePlatform) -> Self {
    match value {
      GamePlatform::Linux => scratch_io::GamePlatform::Linux,
      GamePlatform::Windows => scratch_io::GamePlatform::Windows,
      GamePlatform::Osx => scratch_io::GamePlatform::OSX,
      GamePlatform::Android => scratch_io::GamePlatform::Android,
      GamePlatform::Web => scratch_io::GamePlatform::Web,
      GamePlatform::Flash => scratch_io::GamePlatform::Flash,
      GamePlatform::Java => scratch_io::GamePlatform::Java,
      GamePlatform::UnityWebPlayer => scratch_io::GamePlatform::UnityWebPlayer,
    }
  }
}

#[derive(Subcommand)]
enum Commands {
  #[clap(flatten)]
  WithApi(WithApiCommands),
  #[clap(flatten)]
  WithoutApi(WithoutApiCommands),
}

// These commands will receive a valid API key and its profile
#[derive(Subcommand)]
enum WithApiCommands {
  /// Log in with an API key to use in the other commands
  Auth {
    /// The API key to save
    api_key: String,
  },
  /// Retrieve information about a user
  UserInfo {
    /// The ID of the user to retrieve information about
    user_id: UserID,
  },
  /// Retrieve information about the profile of the current user
  ProfileInfo,
  /// List the games that the user created or that the user is an admin of
  CreatedGames,
  /// List the game keys owned by the user
  OwnedKeys,
  /// List the profile's collections
  ProfileCollections,
  /// Retrieve information about a collection
  CollectionInfo {
    /// The ID of the collection to retrieve information about
    collection_id: CollectionID,
  },
  /// List the games in the given collection
  CollectionGames {
    /// The ID of the collection where the games are located
    collection_id: CollectionID,
  },
  /// Retrieve information about a game given its ID
  GameInfo {
    /// The ID of the game to retrieve information about
    game_id: GameID,
  },
  /// List the uploads available for download for the given game
  GameUploads {
    /// The ID of the game to retrieve information about
    game_id: GameID,
  },
  /// Retrieve information about an upload given its ID
  UploadInfo {
    /// The ID of the upload to retrieve information about
    upload_id: UploadID,
  },
  /// List the builds available for the given upload
  UploadBuilds {
    /// The ID of the upload to retrieve information about
    upload_id: UploadID,
  },
  /// Retrieve information about a build given its ID
  BuildInfo {
    /// The ID of the build to retrieve information about
    build_id: BuildID,
  },
  /// Search for an upgrade path between two builds
  UpgradePath {
    /// The ID of the current build
    current_build_id: BuildID,
    /// The ID of the target build
    target_build_id: BuildID,
  },
  /// Retrieve additional information about the contents of the upload
  UploadScannedArchive {
    /// The ID of the upload to retrieve information about
    upload_id: UploadID,
  },
  /// Retrieve additional information about the contents of the build
  BuildScannedArchive {
    /// The ID of the build to retrieve information about
    build_id: BuildID,
  },
  /// Download the upload with the given ID
  Download {
    /// The ID of the upload to download
    upload_id: UploadID,
    /// The path where the download folder will be placed
    ///
    /// Defaults to ~/Games/{game_name}/
    #[arg(long, env = "SCRATCH_INSTALL_PATH")]
    install_path: Option<PathBuf>,
    /// Skip the hash verification and allow installing modified files (unsafe)
    #[arg(long, env = "SCRATCH_SKIP_HASH_VERIFICATION")]
    skip_hash_verification: bool,
  },
  /// Download a game cover gives its game ID
  DownloadCover {
    /// The ID of the game from which the cover will be downloaded
    game_id: GameID,
    /// The path where the downloaded file will be placed
    #[arg(long, env = "SCRATCH_FOLDER")]
    folder: PathBuf,
    /// The filename of the downloaded cover image (without extension)
    ///
    /// Defaults to "cover"
    #[arg(long, env = "SCRATCH_FILENAME")]
    filename: Option<String>,
    /// Replace the cover image, if one was found
    #[arg(long, env = "SCRATCH_FORCE_DOWNLOAD")]
    force_download: bool,
  },
  /// Remove partially downloaded upload files
  RemovePartialDownload {
    /// The ID of the upload which has been partially downloaded
    upload_id: UploadID,
    /// The path where the download folder has been placed
    ///
    /// Defaults to ~/Games/{game_name}/
    #[arg(long, env = "SCRATCH_INSTALL_PATH")]
    install_path: Option<PathBuf>,
  },
  /// Imports an already installed game given its upload ID and the game folder
  Import {
    /// The ID of the upload to import
    upload_id: UploadID,
    /// The path where the game folder is located
    install_path: PathBuf,
  },
}

// These commands may receive a valid API key, or may not
#[derive(Subcommand)]
enum WithoutApiCommands {
  /// Login with a username and password
  Login {
    /// The username of the user who logs in
    #[arg(long, env = "SCRATCH_USERNAME")]
    username: String,
    /// The password of the user who logs in
    #[arg(long, env = "SCRATCH_PASSWORD")]
    password: String,
    /// The response of the reCAPTCHA (if required)
    #[arg(long, env = "SCRATCH_RECAPTCHA_RESPONSE")]
    recaptcha_response: Option<String>,
  },
  /// Finish logging in with TOTP two-factor authentication
  TOTPVerification {
    /// The two-factor authentication token returned by the login command
    #[arg(long, env = "SCRATCH_TOTP_TOKEN")]
    totp_token: String,
    /// The TOTP two-factor authentication
    #[arg(long, env = "SCRATCH_TOTP_CODE")]
    totp_code: u64,
  },
  /// Remove the saved API key
  Logout,
  /// List the installed games
  Installed,
  /// Get the installed information about an upload given its ID
  InstalledUpload {
    /// The ID of the upload to retrieve information about
    upload_id: UploadID,
  },
  /// Remove a installed upload given its ID
  Remove {
    /// The ID of the upload to remove
    upload_id: UploadID,
  },
  /// Move a installed upload to another game folder
  Move {
    /// The ID of the upload to import
    upload_id: UploadID,
    /// The path where the game folder will be placed
    game_path_dst: PathBuf,
  },
  /// Launchs an installed game given its upload ID and the platform or executable path
  #[command(group(clap::ArgGroup::new("launch_method").required(true).multiple(true)))]
  Launch {
    /// The ID of the upload to launch
    upload_id: UploadID,
    /// The itch manifest's action to call the game with
    ///
    /// Returns an error if the action isn't present in the manifest, or the manifest is missing
    #[arg(long, env = "SCRATCH_LAUNCH_ACTION", group = "launch_method")]
    launch_action: Option<String>,
    /// The platform for which the game binary will be searched
    ///
    /// The itch.io uploads don't specify a game binary, so which file to run will be decided by heuristics.
    ///
    /// The heuristics need to know which platform is the executable they are searching.
    #[arg(long, env = "SCRATCH_PLATFORM", group = "launch_method")]
    platform: Option<GamePlatform>,
    /// Instead of the platform (or in addition to), a executable path can be provided
    #[arg(long, env = "SCRATCH_UPLOAD_EXECUTABLE_PATH", group = "launch_method")]
    upload_executable_path: Option<PathBuf>,
    /// A wrapper command to launch the game with
    #[arg(long, env = "SCRATCH_WRAPPER")]
    wrapper: Option<String>,
    /// The arguments the game will be called with
    ///
    /// The arguments will be split into a vector according to parsing rules of UNIX shell
    #[arg(long, env = "SCRATCH_GAME_ARGUMENTS")]
    game_arguments: Option<String>,
    /// The environment variables that will be added to the game process's environment
    ///
    /// The arguments will be split into key-value pairs using the "=" separator
    #[arg(long, env = "SCRATCH_ENVIRONMENT_VARIABLES")]
    environment_variables: Option<String>,
  },
}

/// Returns a Itch client with the first API key of the vector that is not None
async fn get_itch_client(keys: Vec<Option<String>>) -> Result<ItchClient, String> {
  let api_key = keys.into_iter().find_map(|key| key);

  match api_key {
    None => Err(
      "Error: an itch.io API key is required, either via --api-key, auth, or the login command."
        .to_string(),
    ),
    Some(api_key) => Ok(ItchClient::new(api_key)),
  }
}

fn get_installed_upload_info(
  upload_id: UploadID,
  mut installed_uploads: HashMap<UploadID, InstalledUpload>,
) -> InstalledUpload {
  installed_uploads.remove(&upload_id).unwrap_or_else(|| {
    eprintln_exit!(
      "The given upload id is not installed!: {}",
      upload_id.to_string()
    )
  })
}

fn get_installed_upload_info_ref(
  upload_id: UploadID,
  installed_uploads: &HashMap<UploadID, InstalledUpload>,
) -> &InstalledUpload {
  installed_uploads.get(&upload_id).unwrap_or_else(|| {
    eprintln_exit!(
      "The given upload id is not installed!: {}",
      upload_id.to_string()
    )
  })
}

fn get_installed_upload_info_mut(
  upload_id: UploadID,
  installed_uploads: &mut HashMap<UploadID, InstalledUpload>,
) -> &mut InstalledUpload {
  installed_uploads.get_mut(&upload_id).unwrap_or_else(|| {
    eprintln_exit!(
      "The given upload id is not installed!: {}",
      upload_id.to_string()
    )
  })
}

fn exit_if_already_installed(
  upload_id: UploadID,
  installed_uploads: &HashMap<UploadID, InstalledUpload>,
) {
  if let Some(info) = installed_uploads.get(&upload_id) {
    eprintln_exit!(
      "The game is already installed in: \"{}\"",
      info
        .game_folder
        .join(info.upload_id.to_string())
        .to_string_lossy()
    );
  }
}

// Save a key to the config and print info
async fn auth(client: &ItchClient, config_api_key: &mut Option<String>) {
  // We already checked if the key was valid
  println!("Valid key!");
  *config_api_key = Some(client.get_api_key().to_string());

  // Print user info
  let profile = itch_api::get_profile(client)
    .await
    .unwrap_or_else(|e| eprintln_exit!("{e}"));
  println!("Logged in as: {}", profile.user.get_name());
}

// Login with a username and password, save to the config and print info
async fn login(
  username: &str,
  password: &str,
  recaptcha_response: Option<&str>,
  config_api_key: &mut Option<String>,
) {
  // Create a temporary client and call the login function
  let client = ItchClient::new(String::new());
  let response = itch_api::login(&client, username, password, recaptcha_response)
    .await
    .unwrap_or_else(|e| eprintln_exit!("{e}"));

  // If the login failed, return the corresponding error
  let login_success = match response {
    itch_api::LoginResponse::Success(v) => v,
    itch_api::LoginResponse::CaptchaError(e) => eprintln_exit!("{e}"),
    itch_api::LoginResponse::TOTPError(e) => eprintln_exit!("{e}"),
  };

  // Save the new key to the config
  let new_client = ItchClient::new(login_success.key.key);
  auth(&new_client, config_api_key).await;
}

// Finish login by using two-step verifitaion
async fn totp_verification(totp_token: &str, totp_code: u64, config_api_key: &mut Option<String>) {
  // Create a temporary client and call the totp verification function
  let client = ItchClient::new(String::new());
  let login_success = itch_api::totp_verification(&client, totp_token, totp_code)
    .await
    .unwrap_or_else(|e| eprintln_exit!("{e}"));

  // Save the new key to the config
  let new_client = ItchClient::new(login_success.key.key);
  auth(&new_client, config_api_key).await;
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

/// Print a user info
async fn print_user_info(client: &ItchClient, user_id: UserID) {
  println!(
    "{:#?}",
    itch_api::get_user_info(client, user_id)
      .await
      .unwrap_or_else(|e| eprintln_exit!("{e}"))
  );
}

/// Print the current user info
async fn print_profile_info(client: &ItchClient) {
  println!(
    "{:#?}",
    itch_api::get_profile(client)
      .await
      .unwrap_or_else(|e| eprintln_exit!("{e}"))
  );
}

// List the games that the user created or is an admin of
async fn print_created_games(client: &ItchClient) {
  println!(
    "{:#?}",
    itch_api::get_created_games(client)
      .await
      .unwrap_or_else(|e| eprintln_exit!("{e}"))
  )
}

// List the owned game keys
async fn print_owned_keys(client: &ItchClient) {
  println!(
    "{:#?}",
    itch_api::get_owned_keys(client)
      .await
      .unwrap_or_else(|e| eprintln_exit!("{e}"))
  );
}

// Print information about the user's collections
async fn print_profile_collections(client: &ItchClient) {
  println!(
    "{:#?}",
    itch_api::get_profile_collections(client)
      .await
      .unwrap_or_else(|e| eprintln_exit!("{e}"))
  );
}

// Print information about a collection
async fn print_collection_info(client: &ItchClient, collection_id: CollectionID) {
  println!(
    "{:#?}",
    itch_api::get_collection_info(client, collection_id)
      .await
      .unwrap_or_else(|e| eprintln_exit!("{e}"))
  );
}

// Print the games listed in a collection
async fn print_collection_games(client: &ItchClient, collection_id: CollectionID) {
  println!(
    "{:#?}",
    itch_api::get_collection_games(client, collection_id)
      .await
      .unwrap_or_else(|e| eprintln_exit!("{e}"))
  )
}

// Print information about a game
async fn print_game_info(client: &ItchClient, game_id: GameID) {
  println!(
    "{:#?}",
    itch_api::get_game_info(client, game_id)
      .await
      .unwrap_or_else(|e| eprintln_exit!("{e}"))
  );
}

// Print a game's uploads and platforms information
async fn print_game_uploads(client: &ItchClient, game_id: GameID) {
  let uploads = itch_api::get_game_uploads(client, game_id)
    .await
    .unwrap_or_else(|e| eprintln_exit!("{e}"));
  println!("{uploads:#?}");

  println!("{:#?}", scratch_io::get_game_platforms(&uploads));
}

// Print information about an upload
async fn print_upload_info(client: &ItchClient, upload_id: UploadID) {
  println!(
    "{:#?}",
    itch_api::get_upload_info(client, upload_id)
      .await
      .unwrap_or_else(|e| eprintln_exit!("{e}"))
  );
}

// Print an upload's builds information
async fn print_upload_builds(client: &ItchClient, upload_id: UploadID) {
  println!(
    "{:#?}",
    itch_api::get_upload_builds(client, upload_id)
      .await
      .unwrap_or_else(|e| eprintln_exit!("{e}"))
  );
}

// Print information about a build
async fn print_build_info(client: &ItchClient, build_id: BuildID) {
  println!(
    "{:#?}",
    itch_api::get_build_info(client, build_id)
      .await
      .unwrap_or_else(|e| eprintln_exit!("{e}"))
  );
}

// Print the upgrade path between two builds
async fn print_upgrade_path(
  client: &ItchClient,
  current_build_id: BuildID,
  target_build_id: BuildID,
) {
  println!(
    "{:#?}",
    itch_api::get_upgrade_path(client, current_build_id, target_build_id)
      .await
      .unwrap_or_else(|e| eprintln_exit!("{e}"))
  );
}

// Print the scanned info about an upload
async fn print_scanned_upload(client: &ItchClient, upload_id: UploadID) {
  println!(
    "{:#?}",
    itch_api::get_upload_scanned_archive(client, upload_id)
      .await
      .unwrap_or_else(|e| eprintln_exit!("{e}"))
  );
}

// Print the scanned info about a build
async fn print_scanned_build(client: &ItchClient, build_id: BuildID) {
  println!(
    "{:#?}",
    itch_api::get_build_scanned_archive(client, build_id)
      .await
      .unwrap_or_else(|e| eprintln_exit!("{e}"))
  );
}

// Download a game's upload
async fn download(
  client: &ItchClient,
  upload_id: UploadID,
  dest: Option<&Path>,
  skip_hash_verification: bool,
  installed_uploads: &mut HashMap<UploadID, InstalledUpload>,
) {
  exit_if_already_installed(upload_id, installed_uploads);

  let progress_bar = indicatif::ProgressBar::hidden();
  progress_bar.set_style(
    indicatif::ProgressStyle::default_bar()
      .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}) ({eta})").expect("Invalid indicatif template???")
      .progress_chars("#>-")
  );

  let iu = scratch_io::download_upload(
    client,
    upload_id,
    dest,
    skip_hash_verification,
    |u, g| println!("{g:#?}\n{u:#?}"),
    |download_status| {
      match download_status {
        DownloadStatus::Warning(w) => println!("{w}"),
        DownloadStatus::StartingDownload { bytes_to_download } => {
          println!("Starting download...");
          progress_bar.set_length(bytes_to_download);
          progress_bar.set_draw_target(indicatif::ProgressDrawTarget::stderr());
        }
        DownloadStatus::DownloadProgress { downloaded_bytes } => {
          progress_bar.set_position(downloaded_bytes)
        }
        DownloadStatus::Extract => println!("Extracting archive..."),
      };
    },
    std::time::Duration::from_millis(100),
  )
  .await
  .unwrap_or_else(|e| eprintln_exit!("Error while downloading file!\n{}", e));

  println!(
    "Game upload downloaded to: \"{}\"",
    iu.game_folder
      .join(iu.upload_id.to_string())
      .to_string_lossy()
  );
  installed_uploads.insert(upload_id, iu);
}

// Download a game's cover image
async fn download_cover(
  client: &ItchClient,
  game_id: GameID,
  folder: &Path,
  filename: Option<&str>,
  force_download: bool,
) {
  let cover_path =
    scratch_io::download_game_cover(client, game_id, folder, filename, force_download)
      .await
      .unwrap_or_else(|e| eprintln_exit!("{e}"));

  match cover_path {
    None => eprintln_exit!("The provided game with id: \"{game_id}\" doesn't have a cover image!"),
    Some(p) => println!(
      "Game cover image downloaded to: \"{}\"",
      p.to_string_lossy()
    ),
  }
}

// Remove partially downloaded game files
async fn remove_partial_download(
  client: &ItchClient,
  upload_id: UploadID,
  game_folder: Option<&Path>,
) {
  let was_something_deleted = scratch_io::remove_partial_download(client, upload_id, game_folder)
    .await
    .unwrap_or_else(|e| eprintln_exit!("Couldn't remove partial download: {e}"));

  if was_something_deleted {
    println!("Removed partially downloaded files from upload {upload_id}.");
  } else {
    println!("Didn't found anything to be removed!")
  }
}

// Print a list of the currently installed games
fn print_installed_games(installed_uploads: &mut HashMap<UploadID, InstalledUpload>) {
  for iu in installed_uploads.values_mut() {
    println!("{iu:#?}");
  }
}

// Print the installed info of an upload
async fn print_installed_upload(
  upload_id: UploadID,
  installed_uploads: &mut HashMap<UploadID, InstalledUpload>,
) {
  let iu = get_installed_upload_info_mut(upload_id, installed_uploads);

  println!("{iu:#?}");

  let manifest = scratch_io::get_upload_manifest(upload_id, &iu.game_folder)
    .await
    .unwrap_or_else(|e| eprintln_exit!("Couldn't get the itch manifest of the upload!: {e}"));

  if let Some(m) = manifest {
    println!("{m:#?}");
  }
}

// Import an already installed upload from a folder
async fn import(
  client: &ItchClient,
  upload_id: UploadID,
  game_folder: &Path,
  installed_uploads: &mut HashMap<UploadID, InstalledUpload>,
) {
  exit_if_already_installed(upload_id, installed_uploads);

  let iu = scratch_io::import(client, upload_id, game_folder)
    .await
    .inspect(|ui| {
      println!(
        "Game imported from: \"{}\"",
        ui.game_folder
          .join(ui.upload_id.to_string())
          .to_string_lossy()
      )
    })
    .unwrap_or_else(|e| eprintln_exit!("Error while importing game!\n{}", e));

  installed_uploads.insert(upload_id, iu);
}

// Remove an installed upload from the system
async fn remove_upload(
  upload_id: UploadID,
  installed_uploads: &mut HashMap<UploadID, InstalledUpload>,
) {
  let upload_info = get_installed_upload_info_ref(upload_id, installed_uploads);

  scratch_io::remove(upload_id, &upload_info.game_folder)
    .await
    .unwrap_or_else(|e| eprintln_exit!("Couldn't remove upload!\n{e}"));

  println!(
    "Removed upload {upload_id} from: \"{}\"",
    &upload_info.game_folder.to_string_lossy()
  );

  installed_uploads
    .remove(&upload_id)
    .expect("We have just checked if the key existed, and it did...");
}

// Move an installed upload from a place to another
async fn move_upload(
  upload_id: UploadID,
  dst_game_folder: &Path,
  installed_uploads: &mut HashMap<UploadID, InstalledUpload>,
) {
  let upload_info = get_installed_upload_info_mut(upload_id, installed_uploads);

  let src_game_folder = upload_info.game_folder.to_path_buf();

  upload_info.game_folder = scratch_io::r#move(upload_id, &src_game_folder, dst_game_folder)
    .await
    .unwrap_or_else(|e| eprintln_exit!("Couldn't move upload!\n{e}"));

  println!(
    "Moved upload {upload_id}\n  Source: \"{}\"\n  Destination: \"{}\"",
    src_game_folder.to_string_lossy(),
    upload_info.game_folder.to_string_lossy()
  );
}

// Launch an installed upload
#[allow(clippy::too_many_arguments)]
async fn launch_upload(
  upload_id: UploadID,
  upload_executable_path: Option<PathBuf>,
  launch_action: Option<String>,
  platform: Option<GamePlatform>,
  wrapper: Option<&str>,
  game_arguments: Option<&str>,
  environment_variables: Option<&str>,
  installed_uploads: HashMap<UploadID, InstalledUpload>,
) {
  let upload_info = get_installed_upload_info(upload_id, installed_uploads);
  let game_folder = upload_info.game_folder.to_path_buf();

  let wrapper: Vec<String> = wrapper.map_or(Vec::new(), |w| {
    shell_words::split(w)
      .unwrap_or_else(|e| eprintln_exit!("Couldn't split the wrapper arguments: {w}\n{e}"))
  });

  let game_arguments: Vec<String> = game_arguments.map_or(Vec::new(), |a| {
    shell_words::split(a)
      .unwrap_or_else(|e| eprintln_exit!("Couldn't split the game arguments: {a}\n{e}"))
  });

  let environment_variables: Vec<(String, String)> =
    environment_variables.map_or(Vec::new(), |v| {
      shell_words::split(v)
        .unwrap_or_else(|e| eprintln_exit!("Couldn't split the environment variables: {v}\n{e}"))
        .into_iter()
        .map(|variable| {
          variable.split_once("=")
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .unwrap_or_else(|| {
              eprintln_exit!(
                "Couldn't split the environment variable because it doesn't contain a \"=\": {variable}"
              )
            })
        })
        .collect()
    });

  let launch_method = if let Some(p) = upload_executable_path {
    scratch_io::LaunchMethod::AlternativeExecutable { executable_path: p }
  } else if let Some(action) = launch_action {
    scratch_io::LaunchMethod::ManifestAction {
      manifest_action_name: action,
    }
  } else if let Some(platform) = platform {
    scratch_io::LaunchMethod::Heuristics {
      game_platform: platform.into(),
      game_title: upload_info.game_title.to_string(),
    }
  } else {
    eprintln_exit!(
      r#"A launch method is required! One of: "launch_action", "platform" or "upload_executable_path" must exist!"#
    )
  };

  scratch_io::launch(
    upload_id,
    &game_folder,
    launch_method,
    &wrapper,
    &game_arguments,
    &environment_variables,
    |up, command| {
      println!(
        "Launching game:\n  Executable path: \"{}\"\n  {command:?}",
        up.to_string_lossy()
      )
    },
  )
  .await
  .unwrap_or_else(|e| eprintln_exit!("Couldn't launch: {upload_id}\n{e}"));
}

#[tokio::main]
async fn main() {
  // Read the user commands
  let cli: Cli = Cli::parse();

  // Get the config from the file
  let custom_config_file = cli.config_file;
  let mut config: Config = Config::load_unwrap(custom_config_file.clone()).await;

  // Create itch.io client
  let client = get_itch_client(
    // The api key is:
    vec![
      // 1. If the command is auth, then the provided key
      if let Commands::WithApi(WithApiCommands::Auth { api_key }) = &cli.command {
        Some(api_key.to_string())
      } else {
        None
      },
      // 2. If --api-key is set, then that key
      cli.api_key,
      // 3. If not, then the saved config
      config.api_key.to_owned(),
      // 4. If there isn't a saved config, throw an error
    ],
  )
  .await;

  /**** COMMANDS ****/

  match cli.command {
    Commands::WithApi(command) => {
      let client = client.unwrap_or_else(|e| eprintln_exit!("{e}"));

      match command {
        WithApiCommands::Auth { .. } => {
          auth(&client, &mut config.api_key).await;
          config.save_unwrap(custom_config_file).await;
        }
        WithApiCommands::UserInfo { user_id } => {
          print_user_info(&client, user_id).await;
        }
        WithApiCommands::ProfileInfo => {
          print_profile_info(&client).await;
        }
        WithApiCommands::CreatedGames => {
          print_created_games(&client).await;
        }
        WithApiCommands::OwnedKeys => {
          print_owned_keys(&client).await;
        }
        WithApiCommands::ProfileCollections => {
          print_profile_collections(&client).await;
        }
        WithApiCommands::CollectionInfo { collection_id } => {
          print_collection_info(&client, collection_id).await;
        }
        WithApiCommands::CollectionGames { collection_id } => {
          print_collection_games(&client, collection_id).await;
        }
        WithApiCommands::GameInfo { game_id } => {
          print_game_info(&client, game_id).await;
        }
        WithApiCommands::GameUploads { game_id } => {
          print_game_uploads(&client, game_id).await;
        }
        WithApiCommands::UploadInfo { upload_id } => {
          print_upload_info(&client, upload_id).await;
        }
        WithApiCommands::UploadBuilds { upload_id } => {
          print_upload_builds(&client, upload_id).await;
        }
        WithApiCommands::BuildInfo { build_id } => {
          print_build_info(&client, build_id).await;
        }
        WithApiCommands::UpgradePath {
          current_build_id,
          target_build_id,
        } => {
          print_upgrade_path(&client, current_build_id, target_build_id).await;
        }
        WithApiCommands::UploadScannedArchive { upload_id } => {
          print_scanned_upload(&client, upload_id).await;
        }
        WithApiCommands::BuildScannedArchive { build_id } => {
          print_scanned_build(&client, build_id).await;
        }
        WithApiCommands::Download {
          upload_id,
          install_path,
          skip_hash_verification,
        } => {
          download(
            &client,
            upload_id,
            install_path.as_deref(),
            skip_hash_verification,
            &mut config.installed_uploads,
          )
          .await;
          config.save_unwrap(custom_config_file).await;
        }
        WithApiCommands::DownloadCover {
          game_id,
          folder,
          filename,
          force_download,
        } => {
          download_cover(
            &client,
            game_id,
            &folder,
            filename.as_deref(),
            force_download,
          )
          .await;
        }
        WithApiCommands::RemovePartialDownload {
          upload_id,
          install_path,
        } => {
          remove_partial_download(&client, upload_id, install_path.as_deref()).await;
        }
        WithApiCommands::Import {
          upload_id,
          install_path,
        } => {
          import(
            &client,
            upload_id,
            &install_path,
            &mut config.installed_uploads,
          )
          .await;
          config.save_unwrap(custom_config_file).await;
        }
      }
    }
    Commands::WithoutApi(command) => match command {
      WithoutApiCommands::Login {
        username,
        password,
        recaptcha_response,
      } => {
        login(
          &username,
          &password,
          recaptcha_response.as_deref(),
          &mut config.api_key,
        )
        .await;
        config.save_unwrap(custom_config_file).await;
      }
      WithoutApiCommands::TOTPVerification {
        totp_token,
        totp_code,
      } => {
        totp_verification(&totp_token, totp_code, &mut config.api_key).await;
        config.save_unwrap(custom_config_file).await;
      }
      WithoutApiCommands::Logout => {
        logout(&mut config.api_key);
        config.save_unwrap(custom_config_file).await;
      }
      WithoutApiCommands::Installed => {
        print_installed_games(&mut config.installed_uploads);
      }
      WithoutApiCommands::InstalledUpload { upload_id } => {
        print_installed_upload(upload_id, &mut config.installed_uploads).await;
      }
      WithoutApiCommands::Remove { upload_id } => {
        remove_upload(upload_id, &mut config.installed_uploads).await;
        config.save_unwrap(custom_config_file).await;
      }
      WithoutApiCommands::Move {
        upload_id,
        game_path_dst,
      } => {
        move_upload(upload_id, &game_path_dst, &mut config.installed_uploads).await;
        config.save_unwrap(custom_config_file).await;
      }
      WithoutApiCommands::Launch {
        upload_id,
        launch_action,
        platform,
        upload_executable_path,
        wrapper,
        game_arguments,
        environment_variables,
      } => {
        launch_upload(
          upload_id,
          upload_executable_path,
          launch_action,
          platform,
          wrapper.as_deref(),
          game_arguments.as_deref(),
          environment_variables.as_deref(),
          config.installed_uploads,
        )
        .await;
      }
    },
  }
}
