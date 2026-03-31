use std::{sync::Arc, time::Duration};

use anyhow::{Result, anyhow};
use irc::client::prelude::*;
use tokio::sync::mpsc;
use tokio_stream::StreamExt as _;

use crate::{
    config::Config,
    queue::TimedVecQueue,
    types::{DiscordMessage, IrcMessage, IrcMessageExt as _},
};

const SEND_INTERVAL: Duration = Duration::from_millis(600);
const RECONNECT_DELAY: Duration = Duration::from_secs(15);

pub async fn run(
    config: Arc<Config>,
    mut from_discord: mpsc::Receiver<IrcMessage>,
    to_discord: mpsc::Sender<DiscordMessage>,
) -> Result<()> {
    loop {
        match bridge(&config, &mut from_discord, &to_discord).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                tracing::error!("IRC connection lost: {e}");
                tracing::info!("Reconnecting in {RECONNECT_DELAY:?}");
                tokio::time::sleep(RECONNECT_DELAY).await;
            }
        }
    }
}

async fn bridge(
    config: &Config,
    from_discord: &mut mpsc::Receiver<IrcMessage>,
    to_discord: &mpsc::Sender<DiscordMessage>,
) -> Result<()> {
    let irc_config = irc::client::prelude::Config {
        nickname: Some(config.irc_nick.clone()),
        server: Some(config.irc_server.clone()),
        port: Some(config.irc_port),
        channels: vec![config.irc_channel.clone()],
        use_tls: Some(config.irc_use_tls),
        password: config.irc_password.clone(),
        burst_window_length: Some(8),
        max_messages_in_burst: Some(15),
        ..irc::client::prelude::Config::default()
    };

    tracing::info!(server = %config.irc_server, port = config.irc_port, tls = config.irc_use_tls, "IRC: connecting");

    let mut client = irc::client::Client::from_config(irc_config)
        .await
        .map_err(|e| anyhow!("IRC connect failed: {e}"))?;

    client.identify().map_err(|e| anyhow!("IRC identify failed: {e}"))?;

    tracing::info!(nick = %config.irc_nick, channel = %config.irc_channel, "IRC: connected and identified");

    let irc_tx = client.sender();
    let mut irc_rx = client
        .stream()
        .map_err(|e| anyhow!("Failed to open IRC event stream: {e}"))?;
    let mut queue = TimedVecQueue::new(SEND_INTERVAL);

    loop {
        while let Ok(msg) = from_discord.try_recv() {
            tracing::debug!(nick = %msg.nick, "IRC: queued message from Discord");
            queue.push(msg);
        }

        tokio::select! {
            item = irc_rx.next() => {
                let raw = item
                    .ok_or_else(|| anyhow!("IRC stream closed unexpectedly"))?
                    .map_err(|e| anyhow!("IRC read error: {e}"))?;

                if let Some(discord_msg) = raw.to_discord_message(&config.irc_channel, &config.irc_nick) {
                    tracing::debug!(user = %discord_msg.username, "IRC->Discord: forwarding message");
                    to_discord.send(discord_msg).await
                        .map_err(|_| anyhow!("Discord channel closed — shutting down"))?;
                }
            }

            _ = queue.wait_until_ready() => {
                if let Some(msg) = queue.try_pop() {
                    tracing::debug!(nick = %msg.nick, "Discord->IRC: sending message");
                    let text = format!("<{}> {}", msg.nick, msg.content);
                    for line in text.lines() {
                        let line = if line.len() <= 450 {
                            line
                        } else {
                            let mut end = 450;
                            while !line.is_char_boundary(end) {
                                end -= 1;
                            }
                            tracing::debug!("Discord->IRC: truncated line to {end} bytes");
                            &line[..end]
                        };
                        irc_tx
                            .send(Command::PRIVMSG(config.irc_channel.clone(), line.to_string()))
                            .map_err(|e| anyhow!("IRC send failed: {e}"))?;
                    }
                }
            }

            msg = from_discord.recv() => match msg {
                Some(m) => {
                    tracing::debug!(nick = %m.nick, "IRC: queued message from Discord");
                    queue.push(m);
                }
                None => {
                    tracing::info!("IRC: Discord channel closed, shutting down");
                    return Ok(());
                }
            }
        }
    }
}
