use std::sync::Arc;

use anyhow::{Context as _, Result};
use poise::serenity_prelude as serenity;
use serenity::model::id::ChannelId;
use tokio::sync::{mpsc, RwLock};

use crate::{
    config::Config,
    members::MemberIndex,
    types::{DiscordMessageExt as _, IrcMessage},
};

pub struct Data {
    pub target_channel: ChannelId,
    pub to_irc: mpsc::Sender<IrcMessage>,
    pub member_index: Arc<RwLock<MemberIndex>>,
}

type Error = anyhow::Error;

pub(crate) async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    match event {
        serenity::FullEvent::Ready { data_about_bot, .. } => {
            tracing::info!("Discord receiver: connected as {}", data_about_bot.user.name);

            // Fetch all guild members for target_channel's guild to populate member index
            if let Ok(channel) = data.target_channel.to_channel(&ctx.http).await
                && let Some(guild_channel) = channel.guild()
            {
                let guild_id = guild_channel.guild_id;
                match ctx.http.get_guild_members(guild_id, Some(1000), None).await {
                    Ok(members) => {
                        let mut index = data.member_index.write().await;
                        for member in &members {
                            index.insert_member(member);
                        }
                        tracing::info!(
                            "Discord receiver: populated member index with {} members for guild {}",
                            index.len(),
                            guild_id
                        );
                    }
                    Err(e) => {
                        tracing::warn!("Failed to fetch guild members for guild {guild_id}: {e}");
                    }
                }
            }
        }
        serenity::FullEvent::GuildMemberAddition { new_member } => {
            data.member_index.write().await.insert_member(new_member);
            tracing::debug!(user = %new_member.user.name, "Member index: added new member");
        }
        serenity::FullEvent::GuildMemberUpdate { new: Some(member), .. } => {
            data.member_index.write().await.insert_member(member);
            tracing::debug!(user = %member.user.name, "Member index: updated member");
        }
        serenity::FullEvent::GuildMemberRemoval { user, .. } => {
            data.member_index.write().await.remove_member(user.id);
            tracing::debug!(user = %user.name, "Member index: removed member");
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

            let member_index = data.member_index.read().await;
            let Some(irc_msg) = msg.to_irc_message(&ctx.cache, &member_index) else {
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

pub async fn run(
    config: Arc<Config>,
    to_irc: mpsc::Sender<IrcMessage>,
    member_index: Arc<RwLock<MemberIndex>>,
) -> Result<()> {
    // GUILDS and GUILD_MEMBERS fill the cache with role, channel and member names so mentions
    // resolve to server nicknames on the IRC side. GUILD_MEMBERS is privileged.
    let intents = serenity::GatewayIntents::GUILDS
        | serenity::GatewayIntents::GUILD_MEMBERS
        | serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::MESSAGE_CONTENT;
    let token = config.discord_token.clone();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            event_handler: |ctx, event, framework, data| Box::pin(event_handler(ctx, event, framework, data)),
            ..Default::default()
        })
        .setup({
            let member_index = member_index.clone();
            move |_ctx, _ready, _framework| {
                let member_index = member_index.clone();
                Box::pin(async move {
                    Ok(Data {
                        target_channel: ChannelId::new(config.discord_channel_id),
                        to_irc,
                        member_index,
                    })
                })
            }
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
