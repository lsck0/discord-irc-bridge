use std::sync::Arc;

use anyhow::{Context as _, Result};
use poise::serenity_prelude as serenity;
use serenity::model::id::ChannelId;
use tokio::sync::mpsc;

use crate::{
    config::Config,
    types::{DiscordMessageExt as _, IrcMessage},
};

pub struct Data {
    pub target_channel: ChannelId,
    pub to_irc: mpsc::Sender<IrcMessage>,
}

type Error = anyhow::Error;

pub(crate) async fn event_handler(
    _ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    match event {
        serenity::FullEvent::Ready { data_about_bot, .. } => {
            tracing::info!("Discord receiver: connected as {}", data_about_bot.user.name);
        }
        serenity::FullEvent::Message { new_message: msg } => {
            if msg.channel_id != data.target_channel {
                return Ok(());
            }
            if msg.author.bot {
                tracing::trace!(author = %msg.author.name, "Discord: ignoring bot message");
                return Ok(());
            }
            if msg.webhook_id.is_some() {
                tracing::trace!(author = %msg.author.name, "Discord: ignoring webhook message");
                return Ok(());
            }

            let Some(irc_msg) = msg.to_irc_message() else {
                tracing::debug!(author = %msg.author.name, "Discord: ignoring empty message");
                return Ok(());
            };

            tracing::debug!(author = %msg.author.name, "Discord->IRC: forwarding message");
            if let Err(e) = data.to_irc.send(irc_msg).await {
                tracing::error!("Failed to send to IRC channel: {e}");
            }
        }
        _ => {}
    }

    return Ok(());
}

pub async fn run(config: Arc<Config>, to_irc: mpsc::Sender<IrcMessage>) -> Result<()> {
    let intents = serenity::GatewayIntents::GUILD_MESSAGES | serenity::GatewayIntents::MESSAGE_CONTENT;
    let token = config.discord_token.clone();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            event_handler: |ctx, event, framework, data| Box::pin(event_handler(ctx, event, framework, data)),
            ..Default::default()
        })
        .setup(move |_ctx, _ready, _framework| {
            Box::pin(async move {
                Ok(Data {
                    target_channel: ChannelId::new(config.discord_channel_id),
                    to_irc,
                })
            })
        })
        .build();

    let mut client = serenity::ClientBuilder::new(&token, intents)
        .framework(framework)
        .await
        .context("Failed to create Discord client — check DISCORD_TOKEN")?;

    tracing::info!("Discord receiver: connecting to gateway");

    client.start().await.context("Discord gateway connection failed")?;

    return Ok(());
}
