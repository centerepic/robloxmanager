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
struct UniverseDetails {
    name: String,
}

#[derive(Deserialize)]
struct UniverseResponse {
    data: Vec<UniverseDetails>,
}

/// Resolve a universe ID to its game name. Works unauthenticated.
pub async fn resolve_universe_name(
    client: &RobloxClient,
    universe_id: u64,
) -> Result<String, CoreError> {
    let url = format!("https://games.roblox.com/v1/games?universeIds={universe_id}");
    let resp: UniverseResponse = client.get_json(&url, "").await?;
    resp.data
        .into_iter()
        .next()
        .map(|d| d.name)
        .ok_or_else(|| CoreError::RobloxApi {
            status: 404,
            message: format!("universe {universe_id} not found"),
        })
}

/// Fetch game icon thumbnail URLs for a batch of universe IDs.
/// Returns a vec of `(universe_id, url)` pairs.
pub async fn fetch_game_icons(
    client: &RobloxClient,
    _cookie: &str,
    universe_ids: &[u64],
) -> Result<Vec<(u64, String)>, CoreError> {
    if universe_ids.is_empty() {
        return Ok(vec![]);
    }
    let ids: Vec<String> = universe_ids.iter().map(|id| id.to_string()).collect();
    let ids_param = ids.join(",");
    let url = format!(
        "https://thumbnails.roblox.com/v1/games/icons\
         ?universeIds={ids_param}&returnPolicy=PlaceHolder&size=150x150&format=Png&isCircular=false"
    );

    let resp: ThumbnailResponse = client.get_json(&url, "").await?;

    Ok(universe_ids
        .iter()
        .zip(resp.data.iter())
        .filter_map(|(id, entry)| entry.image_url.clone().map(|url| (*id, url)))
        .collect())
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

// ---------------------------------------------------------------------------
// Share link resolution
// ---------------------------------------------------------------------------

/// Resolve a Roblox share link code (from `/share?code=CODE&type=Server`)
/// into `(place_id, universe_id, link_code, access_code)`.
///
/// Two-step process:
/// 1. POST `apis.roblox.com/sharelinks/v1/resolve-link` to get placeId + linkCode.
/// 2. GET `/games/{placeId}/game?privateServerLinkCode={linkCode}` and scrape
///    the UUID access code from the `joinPrivateGame(...)` JS call.
pub async fn resolve_share_link(
    client: &RobloxClient,
    cookie: &str,
    share_code: &str,
) -> Result<(u64, Option<u64>, String, String), CoreError> {
    use regex::Regex;

    // --- Step 1: Resolve share code → placeId + linkCode via API ---
    let body = serde_json::json!({
        "linkId": share_code,
        "linkType": "Server",
    });
    let resp: serde_json::Value = client
        .post_json(
            "https://apis.roblox.com/sharelinks/v1/resolve-link",
            cookie,
            Some(&body),
        )
        .await?;

    let ps_data = resp
        .get("privateServerInviteData")
        .ok_or_else(|| CoreError::RobloxApi {
            status: 400,
            message: "share link response missing privateServerInviteData".into(),
        })?;

    let place_id = ps_data
        .get("placeId")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| CoreError::RobloxApi {
            status: 400,
            message: "share link response missing placeId".into(),
        })?;

    let link_code = ps_data
        .get("linkCode")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CoreError::RobloxApi {
            status: 400,
            message: "share link response missing linkCode".into(),
        })?
        .to_string();

    let universe_id = ps_data.get("universeId").and_then(|v| v.as_u64());

    let status = ps_data
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");
    if status != "Valid" {
        return Err(CoreError::RobloxApi {
            status: 400,
            message: format!("private server invite status: {status}"),
        });
    }

    tracing::info!("Share link resolved → placeId={place_id}, linkCode={link_code}");

    // --- Step 2: Scrape accessCode (UUID) from the game page ---
    let game_url = format!(
        "https://www.roblox.com/games/{place_id}/game?privateServerLinkCode={link_code}"
    );
    let html = client.get_text(&game_url, cookie).await?;

    let access_re = Regex::new(
        r"Roblox\.GameLauncher\.joinPrivateGame\(\d+\s*,\s*'([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})'"
    ).expect("invalid regex");

    let access_code = access_re
        .captures(&html)
        .and_then(|cap| cap.get(1))
        .ok_or_else(|| CoreError::RobloxApi {
            status: 400,
            message: "could not scrape accessCode from game page".into(),
        })?
        .as_str()
        .to_string();

    tracing::info!("Access code resolved → {access_code}");

    Ok((place_id, universe_id, link_code, access_code))
}

// ---------------------------------------------------------------------------
// GitLab update check
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ReleaseLinks {
    #[serde(rename = "self")]
    self_url: String,
}

#[derive(Deserialize)]
struct GitLabRelease {
    tag_name: String,
    _links: ReleaseLinks,
}

/// Check for a newer release on GitLab. Returns `Some((version, url))` if an
/// update is available, `None` if already on the latest.
pub async fn check_for_updates(current_version: &str) -> Result<Option<(String, String)>, CoreError> {
    let client = reqwest::Client::builder()
        .user_agent("RM-update-check")
        .build()?;

    let release: GitLabRelease = client
        .get("https://gitlab.com/api/v4/projects/centerepic%2Frobloxmanager/releases/permalink/latest")
        .send()
        .await?
        .json()
        .await?;

    let remote = release.tag_name.trim_start_matches('v');
    let local = current_version.trim_start_matches('v');

    if remote != local {
        Ok(Some((remote.to_string(), release._links.self_url)))
    } else {
        Ok(None)
    }
}
