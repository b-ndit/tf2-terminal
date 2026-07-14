use serde::Serialize;

use crate::error::AppResult;

#[derive(Serialize)]
struct DiscordWebhookPayload<'a> {
    content: &'a str,
}

/// Posts `content` to a Discord webhook URL (the standard `{"content":
/// "..."}` webhook-execute contract — no bot token or OAuth needed, just
/// the per-channel webhook URL the user creates and pastes in). The URL
/// is a credential (it grants posting rights), so it's stored in the OS
/// keychain like the Steam/backpack.tf secrets (`docs/DESIGN.md` §11),
/// never in SQLite/config/logs.
pub async fn send(http: &reqwest::Client, webhook_url: &str, content: &str) -> AppResult<()> {
    http.post(webhook_url)
        .json(&DiscordWebhookPayload { content })
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
