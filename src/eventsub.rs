use std::str::FromStr;

use twitch_api::helix::HelixClient;
use url::Url;

use crate::helper::SafeTwitchEventList;
use crate::websocket;

const TEST_WEBSOCKET_URL: &str = "ws://127.0.0.1:8080/ws";

// moderator:read:followers channel:read:subscriptions
pub(crate) async fn run_eventsub_client(event_list: SafeTwitchEventList) {
    let client = HelixClient::<reqwest::Client>::new();

    // let token = twitch_oauth2::UserToken::from_token(
    //     client.get_client(),
    //     std::env::var("TWITCH_USER_TOKEN").unwrap().into(),
    // )
    // .await
    // .unwrap();
    let token: twitch_oauth2::UserToken = twitch_oauth2::UserToken::from_existing_unchecked(
        std::env::var("TWITCH_USER_TOKEN").unwrap(),
        None,
        std::env::var("TWITCH_CLIENT_ID").unwrap(),
        Some(twitch_oauth2::ClientSecret::new(
            std::env::var("TWITCH_CLIENT_SECRET").unwrap(),
        )),
        std::env::var("TWITCH_LOGIN").unwrap().into(),
        std::env::var("TWITCH_USER_ID").unwrap().into(),
        Some(vec![
            twitch_oauth2::Scope::ModeratorReadFollowers,
            twitch_oauth2::Scope::ChannelReadSubscriptions,
        ]),
        Some(std::time::Duration::from_secs(21600)),
    );
    let ws = websocket::WebsocketClient::new(
        None,
        token,
        client,
        From::from("662136860"),
        Url::from_str(TEST_WEBSOCKET_URL).unwrap(),
        event_list,
    );

    ws.run()
        .await
        .expect("Websocket client finished its execution");
}
