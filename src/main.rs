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

    let (label, res) = tokio::select! {
        res = discord_receiver => ("Discord receiver", res),
        res = discord_sender   => ("Discord sender",   res),
        res = irc_bot          => ("IRC bot",           res),
    };

    match res {
        Ok(Ok(())) => tracing::info!("{label} stopped cleanly"),
        Ok(Err(e)) => tracing::error!("{label} stopped: {e:?}"),
        Err(e) => tracing::error!("{label} panicked: {e:?}"),
    }

    // any task exiting during steady-state operation means the bridge is broken;
    // exit non-zero so a restart policy (systemd, docker) picks this up instead of
    // the process idling with two tasks silently stopped.
    return Err(anyhow::anyhow!("{label} exited, shutting down bridge"));
}
