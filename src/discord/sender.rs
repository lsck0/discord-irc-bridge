use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use reqwest::StatusCode;
use serde::Serialize;
use tokio::sync::mpsc;

use crate::{config::Config, queue::TimedVecQueue, types::DiscordMessage};

// Webhooks are documented at 30 req/min. 2 100 ms gives comfortable headroom.
const SEND_INTERVAL: Duration = Duration::from_millis(2_100);

pub async fn run(config: Arc<Config>, mut rx: mpsc::Receiver<DiscordMessage>) -> Result<()> {
    let http = reqwest::Client::new();
    let mut queue = TimedVecQueue::new(SEND_INTERVAL);

    tracing::info!("Discord sender: ready");

    loop {
        while let Ok(msg) = rx.try_recv() {
            queue.push(msg);
        }

        if let Some(msg) = queue.try_pop() {
            tracing::debug!(user = %msg.username, "IRC->Discord: sending webhook");
            match post_to_webhook(&http, &config.discord_webhook_url, &msg).await? {
                SendOutcome::Delivered => {
                    tracing::debug!(user = %msg.username, "IRC->Discord: delivered");
                }
                SendOutcome::RateLimited(retry) => {
                    queue.requeue(msg);
                    queue.backoff(retry);
                }
            }
            continue;
        }

        tokio::select! {
            _ = queue.wait_until_ready() => {}
            msg = rx.recv() => match msg {
                Some(m) => {
                    tracing::debug!(user = %m.username, "Discord sender: queued message");
                    queue.push(m);
                }
                None => {
                    tracing::info!("Discord sender: IRC channel closed, shutting down");
                    return Ok(());
                }
            }
        }
    }
}

enum SendOutcome {
    Delivered,
    RateLimited(Duration),
}

async fn post_to_webhook(http: &reqwest::Client, url: &str, msg: &DiscordMessage) -> Result<SendOutcome> {
    #[derive(Serialize)]
    struct Payload<'a> {
        username: &'a str,
        avatar_url: &'a str,
        content: &'a str,
    }

    let generated_avatar;
    let avatar = match msg.avatar_url.as_deref() {
        Some(url) => url,
        None => {
            let encoded: String = msg
                .username
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                        c.to_string()
                    } else {
                        format!("%{:02X}", c as u32)
                    }
                })
                .collect();
            generated_avatar = format!("https://robohash.org/{encoded}?set=set4");
            &generated_avatar
        }
    };

    let response = http
        .post(url)
        .json(&Payload {
            username: &msg.username,
            avatar_url: avatar,
            content: &msg.content,
        })
        .send()
        .await
        .context("Webhook HTTP request failed")?;

    let status = response.status();

    if status.is_success() {
        return Ok(SendOutcome::Delivered);
    }

    if status == StatusCode::TOO_MANY_REQUESTS {
        let retry_secs = response
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|v| v["retry_after"].as_f64())
            .unwrap_or(5.0);
        tracing::warn!("Webhook rate-limited; backing off {retry_secs:.1}s");
        return Ok(SendOutcome::RateLimited(Duration::from_secs_f64(retry_secs)));
    }

    if status == StatusCode::NOT_FOUND {
        bail!("Webhook URL returned 404 — the webhook has been deleted. Recreate it and update DISCORD_WEBHOOK_URL.");
    }

    if status == StatusCode::UNAUTHORIZED {
        bail!("Webhook URL returned 401 — the webhook token is invalid. Check DISCORD_WEBHOOK_URL.");
    }

    tracing::warn!("Webhook returned unexpected {status}; backing off 5s");
    return Ok(SendOutcome::RateLimited(Duration::from_secs(5)));
}
