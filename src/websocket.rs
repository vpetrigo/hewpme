/// Requires the following permissions:
/// - channel:read:subscriptions
/// - moderator:read:followers
use std::error::Error;
use std::fmt::Formatter;

use tokio_tungstenite::tungstenite;
use tracing::Instrument;
use twitch_api::{
    eventsub::{
        self,
        Event,
        event::websocket::{EventsubWebsocketData, ReconnectPayload, SessionData, WelcomePayload}, Payload,
    },
    HelixClient,
};
use twitch_api::eventsub::channel::{
    ChannelFollowV2, ChannelFollowV2Payload, ChannelSubscribeV1, ChannelSubscribeV1Payload,
};
use twitch_api::types::UserId;
use twitch_oauth2::{TwitchToken, UserToken};
use url::Url;

use crate::helper::SafeTwitchEventList;

pub struct WebsocketClient {
    /// The session id of the websocket connection
    pub session_id: Option<String>,
    /// The token used to authenticate with the Twitch API
    pub token: UserToken,
    /// The client used to make requests to the Twitch API
    pub client: HelixClient<'static, reqwest::Client>,
    /// The user id of the channel we want to listen to
    pub user_id: UserId,
    /// The url to use for websocket
    pub connect_url: Url,
    // pub opts: Arc<crate::Opts>,
    events_list: SafeTwitchEventList,
}

#[derive(Debug)]
pub struct WSError {
    description: String,
}

impl core::fmt::Display for WSError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "error: {}", self.description)
    }
}

impl<T: Error> From<T> for WSError {
    fn from(value: T) -> Self {
        WSError {
            description: value.to_string(),
        }
    }
}

pub type WebSocketStream =
tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

impl WebsocketClient {
    pub fn new(
        session_id: Option<String>,
        token: UserToken,
        client: HelixClient<'static, reqwest::Client>,
        user_id: UserId,
        connect_url: Url,
        events_list: SafeTwitchEventList,
    ) -> Self {
        WebsocketClient {
            session_id,
            token,
            client,
            user_id,
            connect_url,
            events_list,
        }
    }

    /// Connect to the websocket and return the stream
    pub async fn connect(&self) -> Result<WebSocketStream, WSError> {
        tracing::info!("connecting to twitch");
        let config = tungstenite::protocol::WebSocketConfig::default();
        let (socket, _) =
            tokio_tungstenite::connect_async_with_config(&self.connect_url, Some(config), false)
                .await?;

        Ok(socket)
        // socket.Ok(socket)
    }

    /// Run the websocket subscriber
    #[tracing::instrument(name = "subscriber", skip_all, fields())]
    pub async fn run(mut self) -> Result<(), WSError> {
        // Establish the stream
        let mut s = self.connect().await?;
        // Loop over the stream, processing messages as they come in.
        loop {
            if let Some(msg) = futures::StreamExt::next(&mut s).await {
                let span = tracing::info_span!("message received: ", raw_message = ?msg);
                let msg = match msg {
                    Err(tungstenite::Error::Protocol(
                            tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
                        )) => {
                        tracing::warn!(
                            "connection was sent an unexpected frame or was reset, reestablishing it"
                        );
                        s = self.connect().instrument(span).await?;
                        continue;
                    }
                    _ => msg?,
                };

                let result = self.process_message(msg).instrument(span).await;

                if let Err(err) = result {
                    println!("Error: {:?}", err);
                    return Err(err);
                }
            }
        }
    }

    /// Process a message from the websocket
    pub async fn process_message(&mut self, msg: tungstenite::Message) -> Result<(), WSError> {
        match msg {
            tungstenite::Message::Text(s) => {
                tracing::info!("inside text: {s}");
                // Parse the message into a [twitch_api::eventsub::EventsubWebsocketData]
                let result = Event::parse_websocket(&s);

                tracing::info!("parsing result: {result:?}");
                if let Err(e) = result {
                    tracing::error!("parsing error: {e}");
                    return Err(e.into());
                }

                match result.unwrap() {
                    EventsubWebsocketData::Welcome {
                        payload: WelcomePayload { session },
                        ..
                    }
                    | EventsubWebsocketData::Reconnect {
                        payload: ReconnectPayload { session },
                        ..
                    } => {
                        self.process_welcome_message(session).await?;
                        Ok(())
                    }
                    // Here is where you would handle the events you want to listen to
                    EventsubWebsocketData::Notification {
                        metadata: _,
                        payload,
                    } => {
                        self.handle_notification(payload).await;

                        Ok(())
                    }
                    EventsubWebsocketData::Revocation {
                        metadata,
                        payload: _,
                    } => {
                        tracing::info!("got revocation event: {metadata:?}");
                        Ok(())
                    }
                    // eyre::bail!("got revocation event: {metadata:?}"),
                    EventsubWebsocketData::Keepalive {
                        metadata: _,
                        payload: _,
                    } => Ok(()),
                    _ => Ok(()),
                }
            }
            tungstenite::Message::Close(_) => todo!(),
            tungstenite::Message::Ping(_) => Ok(()),
            _ => {
                tracing::warn!("Unhandled case");
                Ok(())
            }
        }
    }

    pub async fn process_welcome_message(&mut self, data: SessionData<'_>) -> Result<(), WSError> {
        self.session_id = Some(data.id.to_string());
        if let Some(ref url) = data.reconnect_url {
            self.connect_url = url.parse()?;
        }
        // check if the token is expired, if it is, request a new token. This only works if using a oauth service for getting a token
        if self.token.is_elapsed() {
            todo!("Token expired - handle that somehow");
            // self.token =
            //     crate::util::get_access_token(self.client.get_client(), &self.opts).await?;
        }

        self.make_eventsub_subscriptions(&data).await?;

        Ok(())
    }

    async fn make_eventsub_subscriptions(&mut self, data: &SessionData<'_>) -> Result<(), WSError> {
        let transport = eventsub::Transport::websocket(data.id.clone());

        println!(
            "Broadcaster: {}, moderator: {}",
            self.user_id.as_str(),
            self.token.user_id.as_str()
        );

        self.client
            .create_eventsub_subscription(
                ChannelFollowV2::new(self.user_id.clone(), self.token.user_id.clone()),
                transport.clone(),
                &self.token,
            )
            .await?;
        self.client
            .create_eventsub_subscription(
                ChannelSubscribeV1::broadcaster_user_id(self.user_id.clone()),
                transport.clone(),
                &self.token,
            )
            .await?;

        Ok(())
    }

    async fn handle_notification(&self, event: Event) {
        match event {
            Event::ChannelFollowV2(payload) => self.handle_channel_follow_event(payload).await,
            Event::ChannelSubscribeV1(payload) => {
                self.handle_channel_subscribe_event(payload).await
            }
            _ => (),
        }
    }

    async fn handle_channel_follow_event(&self, payload: Payload<ChannelFollowV2>) {
        if let eventsub::Message::Notification(ref payload) = payload.message {
            tracing::info!(
                "Got following name: {} {}",
                payload.user_name,
                payload.user_id
            );
            self.put_follower_name(payload).await;
        }
    }

    async fn handle_channel_subscribe_event(&self, payload: Payload<ChannelSubscribeV1>) {
        if let eventsub::Message::Notification(ref payload) = payload.message {
            tracing::info!(
                "Got subscriber name: {} {}",
                payload.user_name,
                payload.user_id
            );
            self.put_subscriber_name(payload).await;
        }
    }

    async fn put_follower_name(&self, payload: &ChannelFollowV2Payload) {
        let follower = if cfg!(feature = "debug") {
            format!("{}{}", payload.user_name, payload.user_id)
        } else {
            format!("{}", payload.user_name)
        };

        self.events_list.add_follower(follower).await;
    }

    async fn put_subscriber_name(&self, payload: &ChannelSubscribeV1Payload) {
        let subscriber = if cfg!(feature = "debug") {
            format!("{}{}", payload.user_name, payload.user_id)
        } else {
            format!("{}", payload.user_name)
        };

        self.events_list.add_subscriber(subscriber).await;
    }
}
