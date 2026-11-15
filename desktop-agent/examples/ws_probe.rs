//! Probe: connects to a running agent's WebSocket and prints every message with the time it came,
//! for a number of seconds. Never prints the token.
//!   cargo run --example ws_probe -- <port> <token> [seconds]

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let port = args.next().unwrap_or_else(|| "8765".into());
    let token = args.next().unwrap_or_default();
    let seconds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(10);

    let (mut socket, _) =
        connect_async(format!("ws://127.0.0.1:{port}/api/v1/ws?token={token}")).await?;
    let started = Instant::now();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    while let Ok(Some(message)) = tokio::time::timeout_at(deadline, socket.next()).await {
        if let Message::Text(text) = message? {
            let text = text.to_string();
            // Long states are cut: only the message type and the start matter here.
            let short: String = text.chars().take(150).collect();
            println!("[{:>5} ms] {short}", started.elapsed().as_millis());
        }
    }
    Ok(())
}
