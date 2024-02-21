use core::time::Duration;
use std::collections::HashMap;
use std::fmt::Formatter;
use std::path::PathBuf;
use std::{env, fs, io};

use chrono::{DateTime, Utc};
use reqwest::{ClientBuilder, IntoUrl};
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;
use twitch_irc::login::UserAccessToken;
use twitch_oauth2::{
    AccessToken, ClientSecret, CsrfToken, RefreshToken, Scope, TwitchToken, UserToken,
    UserTokenBuilder,
};
use url::Url;

use crate::utils::{create_auth_channel, run_auth_server};

pub struct Wrapper {
    token: UserToken,
}

impl Wrapper {
    pub async fn new<T: IntoUrl>(ctx: CreateContext<'_, T>) -> Self {
        Wrapper {
            token: request_user_token(ctx).await,
        }
    }

    pub fn get_user_token(&self) -> &UserToken {
        &self.token
    }
}

#[derive(Serialize, Deserialize)]
pub struct Token {
    #[allow(clippy::struct_field_names)]
    pub access_token: AccessToken,
    #[allow(clippy::struct_field_names)]
    pub refresh_token: Option<RefreshToken>,
    pub created_at: DateTime<Utc>,
    pub valid_till: DateTime<Utc>,
    pub scopes: Option<Vec<Scope>>,
}

impl From<UserToken> for Token {
    fn from(value: UserToken) -> Self {
        value.into()
    }
}

impl From<&UserToken> for Token {
    fn from(value: &UserToken) -> Self {
        let now = chrono::offset::Utc::now();
        let valid_till = now + value.expires_in();

        Self {
            access_token: value.access_token.clone(),
            refresh_token: value.refresh_token.clone(),
            created_at: now,
            valid_till,
            scopes: Some(value.scopes().to_vec()),
        }
    }
}

impl From<UserAccessToken> for Token {
    fn from(value: UserAccessToken) -> Self {
        value.into()
    }
}

impl From<&UserAccessToken> for Token {
    fn from(value: &UserAccessToken) -> Self {
        let valid_till = value
            .expires_at
            .unwrap_or(value.created_at + Duration::from_secs(600));

        Self {
            access_token: value.access_token.clone().into(),
            refresh_token: Some(value.refresh_token.clone().into()),
            created_at: value.created_at,
            valid_till,
            scopes: None,
        }
    }
}

pub struct CreateContext<'a, T: IntoUrl> {
    pub scopes: &'a [Scope],
    pub force_verify: bool,
    pub redirect_url: T,
}

impl<'a, T: IntoUrl> CreateContext<'a, T> {
    pub fn new(scopes: &'a [Scope], force_verify: bool, redirect_url: T) -> Self {
        CreateContext {
            scopes,
            force_verify,
            redirect_url,
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

impl Token {
    pub fn save(&self, out: PathBuf) -> io::Result<()> {
        let file = fs::File::create(out)?;
        let writer = io::BufWriter::new(file);

        serde_json::to_writer(writer, &self)?;

        Ok(())
    }

    pub fn from_file(file: PathBuf) -> io::Result<Self> {
        get_token_from_file(file)
    }

    pub async fn into_user_token(self) -> UserToken {
        let client_secret = env::var("TWITCH_CLIENT_SECRET").unwrap();
        let client = ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("Unable to build a client to send request to Twitch API");
        let token = UserToken::from_existing(
            &client,
            self.access_token,
            self.refresh_token,
            ClientSecret::from(client_secret),
        )
            .await;

        match token {
            Err(e) => panic!("Unable to get token: {e}"),
            Ok(token) => token,
        }
    }
}

fn get_token_from_file(config_file: PathBuf) -> io::Result<Token> {
    // TODO: add handling of existing configuration directory, but file is missed
    let file = fs::File::open(config_file)?;
    let reader = io::BufReader::new(file);

    Ok(serde_json::from_reader(reader)?)
}

fn create_token_context<T: IntoUrl>(ctx: CreateContext<'_, T>) -> UserTokenBuilder {
    let redirect_url = ctx.redirect_url.into_url().expect("Invalid redirect URL");
    let client_id = env::var("TWITCH_CLIENT_ID").unwrap();
    let client_secret = env::var("TWITCH_CLIENT_SECRET").unwrap();
    let mut builder = UserTokenBuilder::new(client_id, client_secret, redirect_url);

    builder = builder.set_scopes(ctx.scopes.to_vec());
    builder = builder.force_verify(ctx.force_verify); // Defaults to false

    builder
}

fn generate_token_url(builder: &mut UserTokenBuilder) -> (Url, CsrfToken) {
    builder.generate_url()
}

fn verify_csrf_token(
    response: &HashMap<String, String>,
    builder: &UserTokenBuilder,
) -> Result<(), Error> {
    let resp_csrf_token = response.get("state");

    if let Some(csrf) = resp_csrf_token {
        return if builder.csrf_is_valid(csrf) {
            Ok(())
        } else {
            Err(Error::CsrfTokenMismatch)
        };
    }

    Err(Error::NoCsrfToken)
}

fn extract_pair<'a>(
    query: &'a HashMap<String, String>,
    key1: &str,
    key2: &str,
) -> (Option<&'a String>, Option<&'a String>) {
    let value1 = query.get(key1);
    let value2 = query.get(key2);

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

async fn request_user_token<T: IntoUrl>(ctx: CreateContext<'_, T>) -> UserToken {
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
    });
    let mut token_context = create_token_context(ctx);
    let (url, csrf_token) = generate_token_url(&mut token_context);
    // Make your user navigate to this URL, for example
    println!("Visit this URL to authorize Twitch access: {url}");
    let auth_response = rx
        .recv()
        .await
        .expect("Unable to get authentication response");
    tokio::task::spawn_blocking(move || {
        handle.block_on(auth_server).unwrap();
    });
    println!("{auth_response:?}, {}", csrf_token.as_str());

    if let Err(e) = verify_csrf_token(&auth_response, &token_context) {
        tracing::error!("CSRF token error: {e}");
        panic!("CSRF token error: {e}");
    }

    match extract_url(&auth_response) {
        Ok((ref state, ref code)) => {
            let client = ClientBuilder::new()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("Unable to build a client to send request to Twitch API");

            token_context
                .get_user_token(&client, state, code)
                .await
                .expect("Failed to get user token from Twitch")
        }
        _ => todo!(),
    }
}
