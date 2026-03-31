#![allow(clippy::needless_return)]

use std::sync::Arc;

use anyhow::Result;
use discord_irc_bridge::{config, discord, irc, types};
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .compact()
        .with_target(false)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("discord_irc_bridge=info,warn")),
        )
        .init();

    let config = Arc::new(config::Config::from_env()?);

    let (discord_to_irc_tx, discord_to_irc_rx) = mpsc::channel::<types::IrcMessage>(256);
    let (irc_to_discord_tx, irc_to_discord_rx) = mpsc::channel::<types::DiscordMessage>(256);

    let discord_receiver = tokio::spawn(discord::receiver::run(config.clone(), discord_to_irc_tx));
    let discord_sender = tokio::spawn(discord::sender::run(config.clone(), irc_to_discord_rx));
    let irc_bot = tokio::spawn(irc::run(config.clone(), discord_to_irc_rx, irc_to_discord_tx));

    tracing::info!(
        "Discord IRC bridge started. Bridging Discord channel {} with IRC channel {}{}",
        config.discord_channel_id,
        config.irc_server,
        config.irc_channel
    );

    tokio::select! {
        res = discord_receiver => tracing::error!("Discord receiver stopped: {:?}", res),
        res = discord_sender   => tracing::error!("Discord sender stopped: {:?}",   res),
        res = irc_bot          => tracing::error!("IRC bot stopped: {:?}",          res),
    }

    return Ok(());
}
