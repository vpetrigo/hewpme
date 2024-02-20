use std::env;

use twitch_irc::login::StaticLoginCredentials;
use twitch_irc::message::ServerMessage::Privmsg;
use twitch_irc::{ClientConfig, SecureTCPTransport, TwitchIRCClient};

use crate::helper::ChattersList;
use crate::utils::get_user_token;

pub async fn run_twitch_irc_client(chatters_list: ChattersList) {
    // default configuration is to join chat as anonymous.
    let token = get_user_token().await;
    let config = ClientConfig::new_simple(StaticLoginCredentials::new(
        token.login.take().to_lowercase(),
        Some(token.access_token.take()),
    ));
    let (mut incoming_messages, client) =
        TwitchIRCClient::<SecureTCPTransport, StaticLoginCredentials>::new(config);

    let client_clone = client.clone();
    // first thing you should do: start consuming incoming messages,
    // otherwise they will back up.
    let join_handle = tokio::spawn(async move {
        while let Some(message) = incoming_messages.recv().await {
            if let Privmsg(ref user_msg) = message {
                chatters_list
                    .lock()
                    .await
                    .insert(user_msg.sender.name.clone());
            }

            tracing::trace!("Received message: {:?}", message);
        }
    });

    // join a channel
    // This function only returns an error if the passed channel login name is malformed,
    // so in this simple case where the channel name is hardcoded we can ignore the potential
    // error with `unwrap`.
    let channel = env::var("TWITCH_CHANNEL").unwrap();
    client.join(channel).unwrap();

    // keep the tokio executor alive.
    // If you return instead of waiting the background task will exit.
    join_handle.await.unwrap();
}
