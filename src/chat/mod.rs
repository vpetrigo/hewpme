/// Requires the following permissions:
/// - channel:read:subscriptions
/// - moderator:read:followers
use std::{env, io};

use async_trait::async_trait;
use twitch_irc::login::{
    RefreshingLoginCredentials, StaticLoginCredentials, TokenStorage, UserAccessToken,
};
use twitch_irc::message::ServerMessage::Privmsg;
use twitch_irc::{ClientConfig, SecureTCPTransport, TwitchIRCClient};
use twitch_oauth2::Scope;

use crate::config;
use crate::helper::ChattersList;
use crate::utils::{Token, TokenCreateContext, TokenHandler};

#[derive(Debug)]
struct ChatTokenStorage;

#[async_trait]
impl TokenStorage for ChatTokenStorage {
    type LoadError = io::Error;
    type UpdateError = io::Error;

    async fn load_token(&mut self) -> Result<UserAccessToken, Self::LoadError> {
        let chat_config = config::get_chat_config_file();
        let token = match Token::from_file(chat_config.clone()) {
            Err(_) => {
                let scopes = [Scope::ChatRead, Scope::ChatEdit];
                let token_create_ctx =
                    TokenCreateContext::new(&scopes, false, config::REDIRECT_URL);
                let token_handler = TokenHandler::new(token_create_ctx).await;

                token_handler.get_user_token().into()
            }
            Ok(token) => token,
        };

        token.save(chat_config)?;

        Ok(UserAccessToken {
            access_token: token.access_token.clone().take(),
            refresh_token: token.refresh_token.clone().unwrap().take(),
            created_at: token.created_at,
            expires_at: Some(token.valid_till),
        })
    }

    async fn update_token(&mut self, token: &UserAccessToken) -> Result<(), Self::UpdateError> {
        let chat_config = config::get_chat_config_file();
        Ok(Token::from(token).save(chat_config)?)
    }
}

pub async fn run_twitch_irc_client(chatters_list: ChattersList) {
    // default configuration is to join chat as anonymous.

    let storage = ChatTokenStorage {};

    let credentials = RefreshingLoginCredentials::init(
        config::get_client_id(),
        config::get_client_secret(),
        storage,
    );
    let config = ClientConfig::new_simple(credentials);
    let (mut incoming_messages, client) = TwitchIRCClient::<
        SecureTCPTransport,
        RefreshingLoginCredentials<ChatTokenStorage>,
    >::new(config);

    let responder = client.clone();
    // first thing you should do: start consuming incoming messages,
    // otherwise they will back up.
    let join_handle = tokio::spawn(async move {
        while let Some(message) = incoming_messages.recv().await {
            if let Privmsg(ref user_msg) = message {
                chatters_list
                    .lock()
                    .await
                    .insert(user_msg.sender.name.clone());
                // TODO: Add some funny commands handling
                responder
                    .say_in_reply_to(user_msg, "Hello".into())
                    .await
                    .unwrap();
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
