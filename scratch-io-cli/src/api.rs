use crate::eprintln_exit;

use clap::Subcommand;
use scratch_io::itch_api::types::{BuildID, CollectionID, GameID, UploadID, UserID};
use scratch_io::itch_api::{ItchClient, endpoints};

// These are the raw itch.io API calls
#[derive(Subcommand)]
pub enum ApiCommand {
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
  /// Request a scoped API subkey for a specific game from the itch.io server,
  /// with permissions scoped to `profile:me`
  GameApiSubkey {
    /// The ID of the game to request a subkey for
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
}

impl ApiCommand {
  pub fn handle_command(self, client: &ItchClient) {
    match self {
      Self::UserInfo { user_id } => {
        println!(
          "{:#?}",
          endpoints::get_user_info(client, user_id).unwrap_or_else(|e| eprintln_exit!("{e}"))
        );
      }
      Self::ProfileInfo => {
        println!(
          "{:#?}",
          endpoints::get_profile(client).unwrap_or_else(|e| eprintln_exit!("{e}"))
        );
      }
      Self::CreatedGames => {
        println!(
          "{:#?}",
          endpoints::get_created_games(client).unwrap_or_else(|e| eprintln_exit!("{e}"))
        )
      }
      Self::OwnedKeys => {
        println!(
          "{:#?}",
          endpoints::get_owned_keys(client).unwrap_or_else(|e| eprintln_exit!("{e}"))
        );
      }
      Self::ProfileCollections => {
        println!(
          "{:#?}",
          endpoints::get_profile_collections(client).unwrap_or_else(|e| eprintln_exit!("{e}"))
        );
      }
      Self::CollectionInfo { collection_id } => {
        println!(
          "{:#?}",
          endpoints::get_collection_info(client, collection_id)
            .unwrap_or_else(|e| eprintln_exit!("{e}"))
        );
      }
      Self::CollectionGames { collection_id } => {
        println!(
          "{:#?}",
          endpoints::get_collection_games(client, collection_id)
            .unwrap_or_else(|e| eprintln_exit!("{e}"))
        )
      }
      Self::GameInfo { game_id } => {
        println!(
          "{:#?}",
          endpoints::get_game_info(client, game_id).unwrap_or_else(|e| eprintln_exit!("{e}"))
        );
      }
      Self::GameApiSubkey { game_id } => {
        println!(
          "{:#?}",
          endpoints::get_game_subkey(client, game_id).unwrap_or_else(|e| eprintln_exit!("{e}"))
        );
      }
      Self::GameUploads { game_id } => {
        let uploads =
          endpoints::get_game_uploads(client, game_id).unwrap_or_else(|e| eprintln_exit!("{e}"));
        println!("{uploads:#?}");

        println!("{:#?}", scratch_io::get_game_platforms(&uploads));
      }
      Self::UploadInfo { upload_id } => {
        println!(
          "{:#?}",
          endpoints::get_upload_info(client, upload_id).unwrap_or_else(|e| eprintln_exit!("{e}"))
        );
      }
      Self::UploadBuilds { upload_id } => {
        println!(
          "{:#?}",
          endpoints::get_upload_builds(client, upload_id).unwrap_or_else(|e| eprintln_exit!("{e}"))
        );
      }
      Self::BuildInfo { build_id } => {
        println!(
          "{:#?}",
          endpoints::get_build_info(client, build_id).unwrap_or_else(|e| eprintln_exit!("{e}"))
        );
      }
      Self::UpgradePath {
        current_build_id,
        target_build_id,
      } => {
        println!(
          "{:#?}",
          endpoints::get_upgrade_path(client, current_build_id, target_build_id)
            .unwrap_or_else(|e| eprintln_exit!("{e}"))
        );
      }
      Self::UploadScannedArchive { upload_id } => {
        println!(
          "{:#?}",
          endpoints::get_upload_scanned_archive(client, upload_id)
            .unwrap_or_else(|e| eprintln_exit!("{e}"))
        );
      }
      Self::BuildScannedArchive { build_id } => {
        println!(
          "{:#?}",
          endpoints::get_build_scanned_archive(client, build_id)
            .unwrap_or_else(|e| eprintln_exit!("{e}"))
        );
      }
    }
  }
}
