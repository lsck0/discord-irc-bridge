use std::collections::HashMap;

use poise::serenity_prelude as serenity;
use serenity::model::{guild::Member, id::UserId};

#[derive(Debug, Default, Clone)]
pub struct MemberIndex {
    name_to_id: HashMap<String, UserId>,
    id_to_display: HashMap<UserId, String>,
}

impl MemberIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_member(&mut self, member: &Member) {
        let display = member
            .nick
            .as_deref()
            .or(member.user.global_name.as_deref())
            .unwrap_or(&member.user.name)
            .to_string();

        self.id_to_display.insert(member.user.id, display);

        // Map lowercase username
        self.name_to_id.insert(member.user.name.to_lowercase(), member.user.id);

        // Map lowercase global_name if present
        if let Some(ref g) = member.user.global_name {
            self.name_to_id.insert(g.to_lowercase(), member.user.id);
        }

        // Map lowercase guild nick if present (highest priority for resolution)
        if let Some(ref n) = member.nick {
            self.name_to_id.insert(n.to_lowercase(), member.user.id);
        }
    }

    pub fn remove_member(&mut self, user_id: UserId) {
        self.id_to_display.remove(&user_id);
        self.name_to_id.retain(|_, id| *id != user_id);
    }

    pub fn get_display_name(&self, user_id: UserId) -> Option<&str> {
        self.id_to_display.get(&user_id).map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.id_to_display.len()
    }

    pub fn is_empty(&self) -> bool {
        self.id_to_display.is_empty()
    }

    /// Resolve IRC mentions into Discord format (`<@USER_ID>`).
    /// Matches `@name` anywhere (where `@` is not preceded by an alphanumeric character),
    /// as well as `name:` or `name,` addressing syntax at the beginning of the text.
    pub fn resolve_irc_to_discord(&self, text: &str) -> String {
        if self.name_to_id.is_empty() {
            return text.to_string();
        }

        let mut out = String::with_capacity(text.len());
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let len = chars.len();
        let mut i = 0;

        // Check if message starts with addressing syntax: `name:` or `name,`
        if let Some((name_end, user_id, punct_char)) = self.check_initial_addressing(text) {
            out.push_str(&format!("<@{user_id}>{punct_char}"));
            while i < len && chars[i].0 < name_end + 1 {
                i += 1;
            }
        }

        while i < len {
            let (_byte_idx, ch) = chars[i];

            if ch == '@' {
                // Ensure '@' is not preceded by alphanumeric character (e.g. email@domain.com)
                let is_preceded_by_alnum = if i > 0 {
                    chars[i - 1].1.is_alphanumeric()
                } else {
                    false
                };

                if !is_preceded_by_alnum {
                    // Collect candidate name
                    let mut j = i + 1;
                    while j < len {
                        let c = chars[j].1;
                        if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' {
                            j += 1;
                        } else {
                            break;
                        }
                    }

                    if j > i + 1 {
                        let start_name = chars[i + 1].0;
                        let end_name = if j < len { chars[j].0 } else { text.len() };
                        let mut candidate = &text[start_name..end_name];

                        // Strip trailing punctuation like '.' or '-' if it was captured
                        while candidate.ends_with('.') || candidate.ends_with('-') {
                            candidate = &candidate[..candidate.len() - 1];
                        }

                        if !candidate.is_empty()
                            && let Some(&user_id) = self.name_to_id.get(&candidate.to_lowercase())
                        {
                            out.push_str(&format!("<@{user_id}>"));
                            let target_byte = start_name + candidate.len();
                            while i < len && chars[i].0 < target_byte {
                                i += 1;
                            }
                            continue;
                        }
                    }
                }
            }

            out.push(ch);
            i += 1;
        }

        out
    }

    fn check_initial_addressing(&self, text: &str) -> Option<(usize, UserId, char)> {
        let colon_pos = text.find(':');
        let comma_pos = text.find(',');
        let (pos, punct) = match (colon_pos, comma_pos) {
            (Some(c), Some(m)) => {
                if c < m {
                    (c, ':')
                } else {
                    (m, ',')
                }
            }
            (Some(c), None) => (c, ':'),
            (None, Some(m)) => (m, ','),
            (None, None) => return None,
        };

        if pos == 0 || pos > 32 {
            return None;
        }

        let candidate = text[..pos].trim();
        if !candidate
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
        {
            return None;
        }

        let user_id = self.name_to_id.get(&candidate.to_lowercase())?;
        Some((pos, *user_id, punct))
    }

    /// Resolve Discord mentions (`<@id>` or `<@!id>`) to readable `@display_name` for IRC.
    pub fn resolve_discord_to_irc(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut progress = 0;
        let mut chars = text.match_indices(['<', '>']).peekable();

        while let Some((idx1, b1)) = chars.next() {
            if b1 == "<"
                && let Some(&(idx2, b2)) = chars.peek()
                && b2 == ">"
            {
                let mention_str = &text[idx1..=idx2];
                let inner = &mention_str[1..mention_str.len() - 1];
                let id_str = inner.strip_prefix("@!").or_else(|| inner.strip_prefix('@'));

                if let Some(id_str) = id_str
                    && let Ok(raw_id) = id_str.parse::<u64>()
                    && let Some(display) = self.get_display_name(UserId::new(raw_id))
                {
                    out.push_str(&text[progress..idx1]);
                    out.push('@');
                    out.push_str(display);
                    progress = idx2 + 1;
                    continue;
                }
            }
        }

        out.push_str(&text[progress..]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_index() -> MemberIndex {
        let mut index = MemberIndex::new();

        // 1. Bot "doorbell"
        let doorbell_member: Member = serde_json::from_str(r#"{
            "user": {
                "id": "1377372383089787034",
                "username": "doorbell",
                "discriminator": "1444",
                "avatar": null
            },
            "roles": [],
            "flags": 0,
            "joined_at": "2025-05-28T19:58:48.957000+00:00",
            "deaf": false,
            "mute": false
        }"#).unwrap();
        index.insert_member(&doorbell_member);

        // 2. User with nick
        let alice_member: Member = serde_json::from_str(r#"{
            "user": {
                "id": "222222222222222222",
                "username": "alice_global",
                "global_name": "Alice Wonderland",
                "discriminator": "0000",
                "avatar": null
            },
            "nick": "alice_irc",
            "roles": [],
            "flags": 0,
            "joined_at": "2025-05-28T19:58:48.957000+00:00",
            "deaf": false,
            "mute": false
        }"#).unwrap();
        index.insert_member(&alice_member);

        index
    }

    #[test]
    fn test_irc_to_discord_doorbell_mention() {
        let index = create_test_index();

        assert_eq!(
            index.resolve_irc_to_discord("@doorbell"),
            "<@1377372383089787034>"
        );
        assert_eq!(
            index.resolve_irc_to_discord("@DOORBELL can someone open up?"),
            "<@1377372383089787034> can someone open up?"
        );
        assert_eq!(
            index.resolve_irc_to_discord("@doorbell: please open"),
            "<@1377372383089787034>: please open"
        );
        assert_eq!(
            index.resolve_irc_to_discord("doorbell: please open"),
            "<@1377372383089787034>: please open"
        );
        assert_eq!(
            index.resolve_irc_to_discord("doorbell, please open"),
            "<@1377372383089787034>, please open"
        );
        assert_eq!(
            index.resolve_irc_to_discord("hello @doorbell!"),
            "hello <@1377372383089787034>!"
        );
        assert_eq!(
            index.resolve_irc_to_discord("email test@doorbell.com"),
            "email test@doorbell.com"
        );
        assert_eq!(
            index.resolve_irc_to_discord("@unknown_user hello"),
            "@unknown_user hello"
        );
    }

    #[test]
    fn test_irc_to_discord_nickname_resolution() {
        let index = create_test_index();

        // Mentioning by guild nick
        assert_eq!(
            index.resolve_irc_to_discord("hey @alice_irc"),
            "hey <@222222222222222222>"
        );
        // Mentioning by username
        assert_eq!(
            index.resolve_irc_to_discord("hey @alice_global"),
            "hey <@222222222222222222>"
        );
    }

    #[test]
    fn test_discord_to_irc_resolution() {
        let index = create_test_index();

        assert_eq!(
            index.resolve_discord_to_irc("Ping <@1377372383089787034>"),
            "Ping @doorbell"
        );
        assert_eq!(
            index.resolve_discord_to_irc("Hello <@222222222222222222>!"),
            "Hello @alice_irc!"
        );
        assert_eq!(
            index.resolve_discord_to_irc("Hello <@!222222222222222222>!"),
            "Hello @alice_irc!"
        );
    }
}
