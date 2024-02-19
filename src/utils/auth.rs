use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use warp::{serve, Filter, Reply};

pub type Sender = mpsc::Sender<HashMap<String, String>>;
pub type Receiver = mpsc::Receiver<HashMap<String, String>>;

pub fn create_auth_channel() -> (Sender, Receiver) {
    mpsc::channel(1)
}

fn with_sender(sender: Sender) -> impl Filter<Extract = (Sender,), Error = Infallible> + Clone {
    warp::any().map(move || sender.clone())
}

fn with_stop_channel(
    cancellation_token: CancellationToken,
) -> impl Filter<Extract = (CancellationToken,), Error = Infallible> + Clone {
    warp::any().map(move || cancellation_token.clone())
}

pub async fn run_auth_server(tx: Sender) {
    let cancel = CancellationToken::new();
    let hello = warp::path!("auth" / "twitch" / "callback")
        .and(warp::query::<HashMap<String, String>>())
        .and(with_sender(tx))
        .and(with_stop_channel(cancel.clone()))
        .and_then(auth_response_handler);
    let server_addr: SocketAddr = "0.0.0.0:3000".parse().unwrap();
    let (_, server) = serve(hello).bind_with_graceful_shutdown(server_addr, async move {
        cancel.cancelled().await;
    });

    server.await;
    tracing::info!("Finish auth server");
}

async fn auth_response_handler(
    query: HashMap<String, String>,
    sender: Sender,
    cancellation_token: CancellationToken,
) -> Result<impl Reply, Infallible> {
    if let Err(e) = sender.send(query).await {
        return Ok(warp::reply::with_status(
            e.to_string(),
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        ));
    }

    cancellation_token.cancel();
    Ok(warp::reply::with_status(
        "Success".to_string(),
        warp::http::StatusCode::OK,
    ))
}
