use core::time::Duration;
use std::collections::HashMap;
use std::fmt::Formatter;
use std::path::PathBuf;
use std::{env, fs, io};

use reqwest::ClientBuilder;
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;
use tokio::sync::{OnceCell, RwLock};
use twitch_oauth2::{
    AccessToken, ClientSecret, CsrfToken, RefreshToken, TwitchToken, UserToken, UserTokenBuilder,
};
use url::Url;

use crate::utils::{create_auth_channel, run_auth_server};

#[derive(Serialize, Deserialize)]
struct Token {
    access_token: AccessToken,
    refresh_token: Option<RefreshToken>,
    expires_in: Duration,
}

impl From<&UserToken> for Token {
    fn from(value: &UserToken) -> Self {
        Self {
            access_token: value.access_token.clone(),
            refresh_token: value.refresh_token.clone(),
            expires_in: value.expires_in(),
        }
    }
}

#[derive(Debug)]
enum Error {
    NoCsrfToken,
    CsrfTokenMismatch,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCsrfToken => write!(f, "No CSRF token in the response"),
            Self::CsrfTokenMismatch => write!(
                f,
                "CSRF token in the response does not match one from the request"
            ),
        }
    }
}

const APP_NAME: &str = "hewpme";
const CONFIG_FILE_NAME: &str = "config.json";

static TOKEN_STORAGE: OnceCell<RwLock<UserToken>> = OnceCell::const_new();

pub async fn get_user_token() -> UserToken {
    TOKEN_STORAGE
        .get_or_init(|| async {
            let bot_config_dir = directories::BaseDirs::new()
                .unwrap()
                .config_dir()
                .join(APP_NAME);

            if bot_config_dir.exists() {
                // load token from the file
                return RwLock::new(restore_token_from_file(bot_config_dir).await);
            }

            RwLock::new(request_user_authentication().await)
        })
        .await
        .read()
        .await
        .clone()
}

pub fn _update_user_token(_token: &str) {
    todo!()
}

fn get_token_from_file(config_dir: PathBuf) -> Token {
    let config_file = config_dir.join(CONFIG_FILE_NAME);
    // TODO: add handling of existing configuration directory, but file is missed
    let file = match fs::File::open(&config_file) {
        Err(e) => panic!("Unable to open file: {e}"),
        Ok(file) => file,
    };
    let reader = io::BufReader::new(file);
    let result = serde_json::from_reader(reader);

    match result {
        Err(e) => panic!("Failed to get token from file: {e}"),
        Ok(token) => token,
    }
}

fn create_token_context() -> UserTokenBuilder {
    let redirect_url = Url::parse("http://localhost:3000/auth/twitch/callback").unwrap();
    let client_id = env::var("TWITCH_CLIENT_ID").unwrap();
    let client_secret = env::var("TWITCH_CLIENT_SECRET").unwrap();
    let mut builder = UserTokenBuilder::new(client_id, client_secret, redirect_url);
    builder = builder.set_scopes(vec![twitch_oauth2::scopes::Scope::UserEdit]);
    builder = builder.force_verify(true); // Defaults to false

    builder
}

fn generate_token_url(builder: &mut UserTokenBuilder) -> (Url, CsrfToken) {
    builder.generate_url()
}

fn verify_csrf_token(response: &HashMap<String, String>, csrf_token: &str) -> Result<(), Error> {
    let resp_csrf_token = response.get("state");

    if resp_csrf_token.is_none() {
        return Err(Error::NoCsrfToken);
    }

    if resp_csrf_token.unwrap() != csrf_token {
        return Err(Error::CsrfTokenMismatch);
    }

    Ok(())
}

fn extract_pair<'a>(
    query: &'a HashMap<String, String>,
    key1: &str,
    key2: &str,
) -> (Option<&'a String>, Option<&'a String>) {
    let value1 = query.get(key1).to_owned();
    let value2 = query.get(key2).to_owned();

    (value1, value2)
}

/// Extract the state and code from the URL a user was redirected to after authorizing the application.
fn extract_url(query: &HashMap<String, String>) -> Result<(String, String), ()> {
    if let (Some(error), Some(error_description)) =
        extract_pair(query, "error", "error_description")
    {
        panic!("Unable to get token {error}, {error_description}")
    } else if let (Some(state), Some(code)) = extract_pair(query, "state", "code") {
        Ok((state.clone(), code.clone()))
    } else {
        Err(())
    }
}

async fn restore_token_from_file(config_dir: PathBuf) -> UserToken {
    let client_secret = env::var("TWITCH_CLIENT_SECRET").unwrap();
    let token = get_token_from_file(config_dir);
    let client = ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Unable to build a client to send request to Twitch API");
    let token = UserToken::from_existing(
        &client,
        token.access_token,
        token.refresh_token,
        ClientSecret::from(client_secret),
    )
    .await;

    return match token {
        Err(e) => panic!("Unable to get token: {e}"),
        Ok(token) => token,
    };
}

async fn request_user_authentication() -> UserToken {
    // no token - retrieve it from Twitch API
    // 1. run auth server
    // 2. generate token URL
    // 3. await response from Twitch API
    // 4. create config dir and config file
    // 5. serialize Token to this file
    let (tx, mut rx) = create_auth_channel();
    let handle = Handle::current();
    let auth_server = handle.spawn(async move {
        run_auth_server(tx).await;
    }); // 1
    let mut token_context = create_token_context();
    let (url, csrf_token) = generate_token_url(&mut token_context); // 2
                                                                    // Make your user navigate to this URL, for example
    println!("Visit this URL to authorize Twitch access: {}", url);
    let auth_response = rx
        .recv()
        .await
        .expect("Unable to get authentication response");
    tokio::task::spawn_blocking(move || {
        handle.block_on(auth_server).unwrap();
    });
    println!("{auth_response:?}, {}", csrf_token.as_str());
    if let Err(e) = verify_csrf_token(&auth_response, csrf_token.as_str()) {
        tracing::error!("CSRF token error: {e}");
        panic!("CSRF token error: {e}");
    }

    match extract_url(&auth_response) {
        Ok((ref state, ref code)) => {
            let client = ClientBuilder::new()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("Unable to build a client to send request to Twitch API");
            let token = token_context
                .get_user_token(&client, state, code)
                .await
                .expect("Failed to get user token from Twitch");
            println!("{token:?}");
            save_token_data(&token);

            token
        }
        _ => todo!(),
    }
}

fn save_token_data(token: &UserToken) {
    let bot_config_dir = directories::BaseDirs::new()
        .unwrap()
        .config_dir()
        .join(APP_NAME);

    if !bot_config_dir.exists() {
        fs::create_dir(&bot_config_dir).expect("Unable to create bot config directory");
    }

    let config_file = bot_config_dir.join(CONFIG_FILE_NAME);
    let file = fs::File::create(config_file).expect("Unable to create bot configuration file");
    let writer = io::BufWriter::new(file);
    let token_to_store: Token = token.into();

    serde_json::to_writer(writer, &token_to_store).expect("Failed to save bot token info");
}
