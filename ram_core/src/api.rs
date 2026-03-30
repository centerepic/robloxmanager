//! Roblox REST API wrappers — avatar thumbnails, presence, place resolution.

use serde::Deserialize;

use crate::auth::RobloxClient;
use crate::error::CoreError;
use crate::models::Presence;

// ---------------------------------------------------------------------------
// Avatar thumbnails
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ThumbnailResponse {
    data: Vec<ThumbnailEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThumbnailEntry {
    image_url: Option<String>,
}

/// Fetch avatar headshot thumbnail URLs for a batch of user IDs.
/// Returns a vec of `(user_id, url)` pairs.
pub async fn fetch_avatars(
    client: &RobloxClient,
    cookie: &str,
    user_ids: &[u64],
) -> Result<Vec<(u64, String)>, CoreError> {
    if user_ids.is_empty() {
        return Ok(vec![]);
    }
    let ids: Vec<String> = user_ids.iter().map(|id| id.to_string()).collect();
    let ids_param = ids.join(",");
    let url = format!(
        "https://thumbnails.roblox.com/v1/users/avatar-headshot\
         ?userIds={ids_param}&size=150x150&format=Png&isCircular=false"
    );

    let resp: ThumbnailResponse = client.get_json(&url, cookie).await?;

    Ok(user_ids
        .iter()
        .zip(resp.data.iter())
        .filter_map(|(id, entry)| entry.image_url.clone().map(|url| (*id, url)))
        .collect())
}

/// Download the actual image bytes for each avatar URL.
/// Returns a vec of `(user_id, raw_bytes)` pairs (skips failures).
pub async fn download_avatar_images(
    client: &RobloxClient,
    cookie: &str,
    avatars: &[(u64, String)],
) -> Vec<(u64, Vec<u8>)> {
    let mut results = Vec::new();
    for (id, url) in avatars {
        match client.get_bytes(url, cookie).await {
            Ok(bytes) => results.push((*id, bytes)),
            Err(e) => tracing::warn!("Failed to download avatar for {id}: {e}"),
        }
    }
    results
}

// ---------------------------------------------------------------------------
// Presence
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresenceResponse {
    user_presences: Vec<PresenceEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresenceEntry {
    user_presence_type: u8,
    place_id: Option<u64>,
    game_id: Option<String>,
    last_location: Option<String>,
}

/// Fetch presence info for multiple user IDs.
pub async fn fetch_presences(
    client: &RobloxClient,
    cookie: &str,
    user_ids: &[u64],
) -> Result<Vec<(u64, Presence)>, CoreError> {
    if user_ids.is_empty() {
        return Ok(vec![]);
    }
    let body = serde_json::json!({ "userIds": user_ids });
    let resp: PresenceResponse = client
        .post_json(
            "https://presence.roblox.com/v1/presence/users",
            cookie,
            Some(&body),
        )
        .await?;

    Ok(user_ids
        .iter()
        .zip(resp.user_presences.iter())
        .map(|(id, p)| {
            (
                *id,
                Presence {
                    user_presence_type: p.user_presence_type,
                    place_id: p.place_id,
                    game_id: p.game_id.clone(),
                    last_location: p.last_location.clone().unwrap_or_default(),
                },
            )
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Place / Universe resolution
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaceDetails {
    name: String,
    universe_id: Option<u64>,
}

/// Resolve a Place ID to its name and universe ID.
pub async fn resolve_place(
    client: &RobloxClient,
    cookie: &str,
    place_id: u64,
) -> Result<(String, Option<u64>), CoreError> {
    let url = format!("https://games.roblox.com/v1/games/multiget-place-details?placeIds={place_id}");
    let details: Vec<PlaceDetails> = client.get_json(&url, cookie).await?;
    let d = details.into_iter().next().ok_or_else(|| {
        CoreError::RobloxApi {
            status: 404,
            message: format!("place {place_id} not found"),
        }
    })?;
    Ok((d.name, d.universe_id))
}

// ---------------------------------------------------------------------------
// Server list (for Job ID joining)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameServer {
    pub id: String,
    pub max_players: u32,
    pub playing: u32,
    pub fps: f32,
    pub ping: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerListResponse {
    data: Vec<GameServer>,
    next_page_cursor: Option<String>,
}

/// Fetch one page of public servers for a place.
pub async fn fetch_servers(
    client: &RobloxClient,
    cookie: &str,
    place_id: u64,
    cursor: Option<&str>,
) -> Result<(Vec<GameServer>, Option<String>), CoreError> {
    let mut url = format!(
        "https://games.roblox.com/v1/games/{place_id}/servers/0?sortOrder=Asc&limit=25"
    );
    if let Some(c) = cursor {
        url.push_str(&format!("&cursor={c}"));
    }
    let resp: ServerListResponse = client.get_json(&url, cookie).await?;
    Ok((resp.data, resp.next_page_cursor))
}
