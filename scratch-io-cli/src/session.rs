use crate::config::Config;
use crate::eprintln_exit;

use clap::Subcommand;
use scratch_io::ItchClient;
use scratch_io::itch_api::{endpoints, oauth};

#[derive(Subcommand)]
pub enum SessionCommand {
  /// Remove the saved API key
  Logout,
  /// Log in with an API key to use in the other commands
  Auth {
    /// The API key to save
    api_key: String,
  },
  /// Log in using OAuth 2.0 with PKCE (generates a URL to authorize in the browser)
  #[clap(subcommand)]
  Oauth(OauthCommand),
}

#[derive(Subcommand)]
pub enum OauthCommand {
  /// Generate an authorization URL and code verifier to start the OAuth flow
  Init,
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

// Check if an api key is valid and print the user info
fn auth(api_key: String, config_api_key: &mut Option<String>) {
  // Create a client using the provided key
  let client = ItchClient::new(api_key);

  // Try to get the user info to check if the key is valid
  let profile = endpoints::get_profile(&client).unwrap_or_else(|e| eprintln_exit!("{e}"));

  // If an error hasn't been thrown, the API key is valid
  *config_api_key = Some(client.api_key().to_string());

  // Print user info
  println!(
    "Valid key!
Logged in as: {}",
    profile.user.get_name()
  );
}

fn oauth_init() {
  let oauth = oauth::get_oauth_url();

  println!(
    "Open this URL in your browser to authorize:
  {}
  
code_verifier:
  {}",
    oauth.url,
    oauth.code_verifier.as_str(),
  );
}

impl SessionCommand {
  pub fn handle_command(self, config: &mut Config) {
    match self {
      Self::Logout => logout(&mut config.api_key),
      Self::Auth { api_key } => auth(api_key, &mut config.api_key),
      Self::Oauth(OauthCommand::Init) => oauth_init(),
    }
  }
}
