use core::str::FromStr;
use std::env;

use twitch_api::helix::HelixClient;
use twitch_api::types::UserId;
use twitch_oauth2::{Scope, UserToken};
use url::Url;

use crate::helper::SafeTwitchEventList;
use crate::utils::{CreateContext, Token, Wrapper};
use crate::{config, websocket};

const TEST_WEBSOCKET_URL: &str = "ws://127.0.0.1:8080/ws";

// moderator:read:followers channel:read:subscriptions
pub(crate) async fn run_eventsub_client(event_list: SafeTwitchEventList) {
    let config_file = config::get_eventsub_config_file();
    let token = match Token::from_file(config_file.clone()) {
        Err(_) => {
            let scopes = [
                Scope::ModeratorReadFollowers,
                Scope::ModeratorManageBannedUsers,
                Scope::ChannelReadSubscriptions,
            ];
            let token_create_ctx = CreateContext::new(&scopes, false, config::REDIRECT_URL);
            let token_handler = Wrapper::new(token_create_ctx).await;

            token_handler.get_user_token().into()
        }
        Ok(token) => token,
    };

    token
        .save(config_file)
        .expect("Unable to save EventSub token");
    let token = token.into_user_token().await;
    let client = HelixClient::<reqwest::Client>::new();
    let channel_name =
        env::var("TWITCH_CHANNEL").expect("Please specify Twitch channel name to connect to");

    let connection_url = if cfg!(feature = "debug") {
        Url::from_str(TEST_WEBSOCKET_URL).unwrap()
    } else {
        Url::from_str(twitch_api::TWITCH_EVENTSUB_WEBSOCKET_URL.as_str()).unwrap()
    };
    let user_id: UserId = if cfg!(feature = "debug") {
        From::from("123456")
    } else {
        get_user_id(&client, &token, &channel_name).await
    };

    let ws = websocket::WSlient::new(None, token, client, user_id, connection_url, event_list);

    ws.run()
        .await
        .expect("Websocket client finished its execution");
}

async fn get_user_id<'a, C: 'a>(
    client: &'a HelixClient<'a, C>,
    token: &UserToken,
    user_name: &str,
) -> UserId
    where
        C: twitch_api::HttpClient,
{
    match client.get_user_from_login(user_name, token).await {
        Ok(user_id) => user_id.unwrap().id,
        Err(e) => panic!("Unable to get User ID from Twitch: {e}"),
    }
}

pub async fn ban_user(user_id: &str, reason: &str) {
    let client = HelixClient::<reqwest::Client>::new();
    let config_file = config::get_eventsub_config_file();
    let token = Token::from_file(config_file.clone()).expect("Unable to get token from file");
    let duration_sec = 30;
    let token = token.into_user_token().await;

    if let Err(e) = client
        .ban_user(
            user_id,
            reason,
            duration_sec,
            token.user_id.clone(),
            token.user_id.clone(),
            &token,
        )
        .await
    {
        tracing::warn!("Unable to ban user {user_id}: {e}");
    }
}
