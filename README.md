<div>
    <h1 align="center">
        Discord IRC Bridge
    </h1>
    <h3 align="center">
        bi-directional syncing of one discord channel with one irc channel
    </h3>
</div>

## Setup

### Discord Bot (for reading messages)

1. Go to the [Discord Developer Portal](https://discord.com/developers/applications) and create a new application
2. Go to **Bot** → click **Reset Token** → copy the token — this is your `DISCORD_TOKEN`
3. Under **Privileged Gateway Intents**, enable **Message Content Intent**
4. Go to **OAuth2** → **URL Generator** → select the **bot** scope → select **Read Message History** and **View Channels** permissions
5. Open the generated URL to invite the bot to your server

### Discord Webhook (for sending messages)

1. In your Discord server, go to the channel you want to bridge
2. Click **Edit Channel** → **Integrations** → **Webhooks** → **New Webhook**
3. Copy the webhook URL — this is your `DISCORD_WEBHOOK_URL`

### Channel ID

1. Enable Developer Mode in Discord (**Settings** → **Advanced** → **Developer Mode**)
2. Right-click the channel → **Copy Channel ID** — this is your `DISCORD_CHANNEL_ID`

## Usage

```yaml
# compose.yml
services:
  bridge:
    image: ghcr.io/lsck0/discord-irc-bridge:latest
    environment:
      DISCORD_TOKEN: "your-bot-token"
      DISCORD_WEBHOOK_URL: "https://discord.com/api/webhooks/..."
      DISCORD_CHANNEL_ID: "123456789012345678"
      IRC_SERVER: "irc.libera.chat"
      IRC_PORT: "6697"
      IRC_CHANNEL: "#your-channel"
      IRC_NICK: "bridge-bot"
      IRC_PASSWORD: "optional-server-password" # optional
      IRC_USE_TLS: "true"
      RUST_LOG: "discord_irc_bridge=debug,warn"
```

```sh
docker compose up -d
```
