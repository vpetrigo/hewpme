const APP_NAME: &str = "hewpme";

use std::path::PathBuf;
use std::{env, fs};

use directories::BaseDirs;

pub const REDIRECT_URL: &str = "http://localhost:3000/auth/twitch/callback";
pub const CHAT_CONFIG_FILE_NAME: &str = "chat.json";
pub const EVENTSUB_CONFIG_FILE_NAME: &str = "eventsub.json";

/// # Panics
///
/// Will panic if application directory cannot be created
#[must_use]
pub fn get_app_directory_path() -> PathBuf {
    let app_dir = BaseDirs::new().unwrap().config_dir().join(APP_NAME);

    if !app_dir.exists() {
        fs::create_dir(&app_dir).expect("Unable to create bot config directory");
    }

    app_dir
}

#[must_use]
pub fn get_eventsub_config_file() -> PathBuf {
    get_app_directory_path().join(EVENTSUB_CONFIG_FILE_NAME)
}

#[must_use]
pub fn get_chat_config_file() -> PathBuf {
    get_app_directory_path().join(CHAT_CONFIG_FILE_NAME)
}

/// # Panics
///
/// Will panic `TWITCH_CLIENT_ID` environment variable is not set
#[must_use]
pub fn get_client_id() -> String {
    env::var("TWITCH_CLIENT_ID").unwrap()
}

/// # Panics
///
/// Will panic `TWITCH_CLIENT_SECRET` environment variable is not set
#[must_use]
pub fn get_client_secret() -> String {
    env::var("TWITCH_CLIENT_SECRET").unwrap()
}
