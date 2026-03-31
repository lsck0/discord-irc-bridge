#[derive(Debug, Clone)]
pub struct DiscordMessage {
    pub username: String,
    pub content: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct IrcMessage {
    pub nick: String,
    pub content: String,
}

// ── Trait extensions on crate message types ──────────────────────────────────

pub trait DiscordMessageExt {
    fn to_irc_message(&self) -> Option<IrcMessage>;
}

impl DiscordMessageExt for poise::serenity_prelude::Message {
    fn to_irc_message(&self) -> Option<IrcMessage> {
        let content = self.content.trim();
        if content.is_empty() {
            return None;
        }
        return Some(IrcMessage {
            nick: self.author.name.clone(),
            content: normalize_for_irc(content),
        });
    }
}

pub trait IrcMessageExt {
    fn to_discord_message(&self, channel: &str, own_nick: &str) -> Option<DiscordMessage>;
}

impl IrcMessageExt for irc::proto::Message {
    fn to_discord_message(&self, channel: &str, own_nick: &str) -> Option<DiscordMessage> {
        let irc::proto::Command::PRIVMSG(ref target, ref text) = self.command else {
            return None;
        };
        if target != channel {
            return None;
        }
        let nick = self.source_nickname()?;
        if nick == own_nick {
            return None;
        }

        let content = match text.strip_prefix("\x01ACTION").and_then(|s| s.strip_suffix('\x01')) {
            Some(action) => format!("_{}_", action.trim()),
            None => text.clone(),
        };

        return Some(DiscordMessage {
            username: nick.to_string(),
            content,
            avatar_url: None,
        });
    }
}

// ── Private helpers ──────────────────────────────────────────────────────────

fn normalize_for_irc(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut chars = content.char_indices().peekable();

    while let Some((i, ch)) = chars.next() {
        if ch == '<' {
            let rest = &content[i..];
            if let Some(end) = rest.find('>') {
                let inner = &rest[1..end];
                let name_part = inner.strip_prefix(':').or_else(|| inner.strip_prefix("a:"));

                if let Some(name_and_id) = name_part {
                    let name = name_and_id.split(':').next().unwrap_or(name_and_id);
                    out.push(':');
                    out.push_str(name);
                    out.push(':');
                    let skip_to = i + end + 1;
                    while chars.peek().is_some_and(|(j, _)| *j < skip_to) {
                        chars.next();
                    }
                    continue;
                }
            }
        }
        out.push(ch);
    }

    return out;
}
