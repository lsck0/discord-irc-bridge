use std::env;

use anyhow::{Context, Result};

pub struct Config {
    pub discord_token: String,
    pub discord_webhook_url: String,
    pub discord_channel_id: u64,

    pub irc_server: String,
    pub irc_port: u16,
    pub irc_channel: String,
    pub irc_nick: String,
    pub irc_password: Option<String>,
    pub irc_use_tls: bool,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        return Ok(Config {
            discord_token: env::var("DISCORD_TOKEN").context("DISCORD_TOKEN is required")?,
            discord_webhook_url: env::var("DISCORD_WEBHOOK_URL").context("DISCORD_WEBHOOK_URL is required")?,
            discord_channel_id: env::var("DISCORD_CHANNEL_ID")
                .context("DISCORD_CHANNEL_ID is required")?
                .parse()
                .context("DISCORD_CHANNEL_ID must be a valid u64 snowflake")?,

            irc_server: env::var("IRC_SERVER").context("IRC_SERVER is required")?,
            irc_port: env::var("IRC_PORT")
                .context("IRC_PORT is required")?
                .parse()
                .context("IRC_PORT must be a valid port number (u16)")?,
            irc_channel: env::var("IRC_CHANNEL").context("IRC_CHANNEL is required")?,
            irc_nick: env::var("IRC_NICK").context("IRC_NICK is required")?,
            irc_password: env::var("IRC_PASSWORD").ok(),
            irc_use_tls: env::var("IRC_USE_TLS")
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
        });
    }
}
