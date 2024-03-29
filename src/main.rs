use helper::create_new_chatters_list;

use crate::chat::run_twitch_irc_client;
use crate::eventsub::run_eventsub_client;
use crate::helper::create_new_twitch_event_list;

mod chat;
pub mod config;
mod eventsub;
mod helper;
mod server;
mod utils;
mod websocket;

fn main() {
    let chatters_list = create_new_chatters_list();
    let events_list = create_new_twitch_event_list();
    let events_list2 = events_list.clone();
    let client_list = chatters_list.clone();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tracing_subscriber::fmt::init();

    let webserver_handle = rt.spawn(async move {
        server::run_server(chatters_list, events_list).await;
    });
    let eventsub_client_handler = rt.spawn(async move {
        run_eventsub_client(events_list2).await;
    });
    let twitch_client_handler = rt.spawn(async move {
        run_twitch_irc_client(client_list).await;
    });

    for handle in [
        eventsub_client_handler,
        twitch_client_handler,
        webserver_handle,
    ] {
        rt.block_on(handle).unwrap();
    }
}
