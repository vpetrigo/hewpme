use std::{env, thread};

use twitch_irc::login::StaticLoginCredentials;
use twitch_irc::message::ServerMessage::Privmsg;
use twitch_irc::{ClientConfig, SecureTCPTransport, TwitchIRCClient};

use helper::{create_new_chatters_list, ChattersList};

mod helper;
mod server;

fn main() {
    let chatters_list = create_new_chatters_list();
    let client_list = chatters_list.clone();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let webserver_handle = thread::spawn(|| server::run_server(chatters_list));
    let twitch_client_handler = rt.spawn(async move {
        run_twitch_irc_client(client_list).await;
    });

    rt.block_on(twitch_client_handler).unwrap();
    webserver_handle
        .join()
        .expect("Unable to wait for the thread");
}

async fn run_twitch_irc_client(chatters_list: ChattersList) {
    tracing_subscriber::fmt::init();

    // default configuration is to join chat as anonymous.
    let config = ClientConfig::default();
    let (mut incoming_messages, client) =
        TwitchIRCClient::<SecureTCPTransport, StaticLoginCredentials>::new(config);

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

            tracing::info!("Received message: {:?}", message);
        }
    });

    // join a channel
    // This function only returns an error if the passed channel login name is malformed,
    // so in this simple case where the channel name is hardcoded we can ignore the potential
    // error with `unwrap`.
    // client.join("vpetrigo".to_owned()).unwrap();
    let channel = env::var("TWITCH_CHANNEL").unwrap();
    client.join(channel).unwrap();

    // keep the tokio executor alive.
    // If you return instead of waiting the background task will exit.
    join_handle.await.unwrap();
}
