//! Bridge between the synchronous `egui` update loop and the `tokio` async runtime.
//!
//! All heavyweight operations (network, file I/O, process spawning) are dispatched
//! as [`BackendCommand`] messages to a background `tokio` runtime. Results come
//! back as [`BackendEvent`] through an mpsc channel polled each frame.

use eframe::egui;
use ram_core::auth::RobloxClient;
use ram_core::models::{Account, AccountStore, Presence};
use ram_core::{api, crypto, process, CoreError};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::{error, info};

// ---------------------------------------------------------------------------
// Commands (UI → Backend)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub enum BackendCommand {
    /// Validate a cookie and add the account.
    AddAccount {
        cookie: String,
        password: String,
        use_credential_manager: bool,
    },
    /// Remove an account by user ID.
    RemoveAccount { user_id: u64 },
    /// Refresh avatar URLs for all accounts.
    RefreshAvatars { user_ids: Vec<u64>, cookie: String },
    /// Refresh presence for all accounts.
    RefreshPresence { user_ids: Vec<u64>, cookie: String },
    /// Launch the game for an account.
    LaunchGame {
        cookie: String,
        place_id: u64,
        job_id: Option<String>,
        link_code: Option<String>,
        access_code: Option<String>,
        multi_instance: bool,
        kill_background: bool,
        privacy_mode: bool,
    },
    /// Launch the game, decrypting the cookie on the backend side.
    LaunchGameEncrypted {
        user_id: u64,
        encrypted_cookie: Option<String>,
        password: String,
        use_credential_manager: bool,
        place_id: u64,
        job_id: Option<String>,
        link_code: Option<String>,
        access_code: Option<String>,
        multi_instance: bool,
        kill_background: bool,
        privacy_mode: bool,
    },
    /// Save the account store to disk.
    SaveStore {
        store: AccountStore,
        path: PathBuf,
        password: String,
    },
    /// Load the account store from disk.
    LoadStore { path: PathBuf, password: String },
    /// Kill all Roblox instances.
    KillAll,
    /// Refresh avatars + presence for all accounts, decrypting a cookie on this side.
    RefreshAll {
        user_ids: Vec<u64>,
        /// The first account's encrypted cookie (or None if credential manager).
        first_user_id: u64,
        encrypted_cookie: Option<String>,
        password: String,
        use_credential_manager: bool,
    },
    /// Lightweight presence-only refresh for a subset of visible accounts.
    RefreshPresenceOnly {
        user_ids: Vec<u64>,
        first_user_id: u64,
        encrypted_cookie: Option<String>,
        password: String,
        use_credential_manager: bool,
    },
    /// Launch multiple accounts into the same game sequentially.
    BulkLaunchEncrypted {
        /// (user_id, encrypted_cookie) pairs for each account.
        accounts: Vec<(u64, Option<String>)>,
        password: String,
        use_credential_manager: bool,
        place_id: u64,
        job_id: Option<String>,
        link_code: Option<String>,
        access_code: Option<String>,
        multi_instance: bool,
        kill_background: bool,
        privacy_mode: bool,
    },
    /// Re-validate all accounts' cookies automatically.
    RevalidateAll {
        /// (user_id, encrypted_cookie) pairs for each account.
        accounts: Vec<(u64, Option<String>)>,
        password: String,
        use_credential_manager: bool,
    },
    /// Arrange all Roblox windows in a tiled grid.
    ArrangeWindows,
    /// Check GitLab for a newer release.
    CheckForUpdates { current_version: String },
    /// Resolve a place ID to its name (for private server auto-check).
    ResolvePlace {
        place_id: u64,
        universe_id: Option<u64>,
        /// Index into the private_servers list so the UI can update the right entry.
        index: usize,
    },
    /// Resolve a share link code into (place_id, link_code) via the Roblox API.
    ResolveShareLink {
        share_code: String,
        server_name: String,
        /// The encrypted cookie + auth info needed for the authenticated API call.
        first_user_id: u64,
        encrypted_cookie: Option<String>,
        password: String,
        use_credential_manager: bool,
    },
}

// ---------------------------------------------------------------------------
// Events (Backend → UI)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub enum BackendEvent {
    /// An account was validated and is ready to be added.
    AccountValidated {
        account: Box<Account>,
        encrypted_cookie: Option<String>,
    },
    /// Account removed.
    AccountRemoved { user_id: u64 },
    /// Avatar URLs fetched.
    AvatarsUpdated(Vec<(u64, String)>),
    /// Raw avatar image bytes downloaded.
    AvatarImagesReady(Vec<(u64, Vec<u8>)>),
    /// Presences fetched.
    PresencesUpdated(Vec<(u64, Presence)>),
    /// Game launched successfully.
    GameLaunched,
    /// Store saved.
    StoreSaved,
    /// Store loaded from disk.
    StoreLoaded(AccountStore),
    /// All Roblox instances killed (count).
    Killed(usize),
    /// Progress update during a bulk launch (launched_so_far, total).
    BulkLaunchProgress { launched: usize, total: usize },
    /// Bulk launch completed.
    BulkLaunchComplete { launched: usize, failed: usize },
    /// Account cookie revalidation result.
    AccountRevalidated {
        user_id: u64,
        valid: bool,
        username: String,
        display_name: String,
    },
    /// An error occurred during a background operation.
    Error(String),
    /// Windows were arranged.
    WindowsArranged,
    /// A newer version is available on GitLab.
    UpdateAvailable { version: String, url: String },
    /// Place name resolved for a private server entry.
    PlaceResolved {
        index: usize,
        place_name: String,
        place_id: u64,
        icon_bytes: Option<Vec<u8>>,
    },
    /// Share link resolved — contains the actual place_id, link_code, access_code, and server name.
    ShareLinkResolved {
        server_name: String,
        place_id: u64,
        universe_id: Option<u64>,
        link_code: String,
        access_code: String,
    },
    /// Share link resolution failed.
    ShareLinkFailed(String),
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

pub struct BackendBridge {
    pub cmd_tx: mpsc::UnboundedSender<BackendCommand>,
    pub evt_rx: mpsc::UnboundedReceiver<BackendEvent>,
    repaint_ctx: Option<egui::Context>,
}

impl BackendBridge {
    /// Spawn the `tokio` runtime on a dedicated thread and return the bridge.
    pub fn spawn() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<BackendCommand>();
        let (evt_tx, evt_rx) = mpsc::unbounded_channel::<BackendEvent>();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime");
            rt.block_on(backend_loop(cmd_rx, evt_tx));
        });

        Self { cmd_tx, evt_rx, repaint_ctx: None }
    }

    /// Give the bridge an egui context so it can request repaints when events arrive.
    pub fn set_repaint_ctx(&mut self, ctx: egui::Context) {
        if self.repaint_ctx.is_none() {
            self.repaint_ctx = Some(ctx);
        }
    }

    /// Send a command to the backend (non-blocking).
    pub fn send(&self, cmd: BackendCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    /// Drain all pending events. Call once per frame.
    pub fn poll(&mut self) -> Vec<BackendEvent> {
        let mut events = Vec::new();
        while let Ok(evt) = self.evt_rx.try_recv() {
            events.push(evt);
        }
        if !events.is_empty() {
            if let Some(ctx) = &self.repaint_ctx {
                ctx.request_repaint();
            }
        }
        events
    }
}

// ---------------------------------------------------------------------------
// Async event loop
// ---------------------------------------------------------------------------

async fn backend_loop(
    mut rx: mpsc::UnboundedReceiver<BackendCommand>,
    tx: mpsc::UnboundedSender<BackendEvent>,
) {
    let client = RobloxClient::default();

    while let Some(cmd) = rx.recv().await {
        let client = client.clone();
        let tx = tx.clone();

        // Each command runs as its own spawned task for concurrency.
        tokio::spawn(async move {
            match handle_command(cmd, &client, &tx).await {
                Ok(evt) => {
                    let _ = tx.send(evt);
                }
                Err(e) => {
                    error!("Backend error: {e}");
                    let _ = tx.send(BackendEvent::Error(e.to_string()));
                }
            }
        });
    }
}

async fn handle_command(
    cmd: BackendCommand,
    client: &RobloxClient,
    tx: &mpsc::UnboundedSender<BackendEvent>,
) -> Result<BackendEvent, CoreError> {
    match cmd {
        BackendCommand::AddAccount {
            cookie,
            password,
            use_credential_manager,
        } => {
            let (user_id, username, display_name) = client.validate_cookie(&cookie).await?;
            let mut account = Account::new(user_id, username, display_name);

            let encrypted = if use_credential_manager {
                crypto::credential_store(user_id, &cookie)?;
                None
            } else {
                Some(crypto::encrypt_cookie(&cookie, &password)?)
            };
            account.encrypted_cookie = encrypted.clone();
            account.last_validated = Some(chrono::Utc::now());

            // Fetch avatar URL and image bytes immediately after validation
            if let Ok(avatars) = api::fetch_avatars(client, &cookie, &[user_id]).await {
                if let Some((_, url)) = avatars.first() {
                    account.avatar_url = url.clone();
                }
                let images = api::download_avatar_images(client, &cookie, &avatars).await;
                if !images.is_empty() {
                    let _ = tx.send(BackendEvent::AvatarImagesReady(images));
                }
            }

            info!("Validated account {} ({})", account.username, user_id);
            Ok(BackendEvent::AccountValidated {
                account: Box::new(account),
                encrypted_cookie: encrypted,
            })
        }
        BackendCommand::RemoveAccount { user_id } => {
            // Best-effort delete from credential manager
            let _ = crypto::credential_delete(user_id);
            Ok(BackendEvent::AccountRemoved { user_id })
        }
        BackendCommand::RefreshAvatars { user_ids, cookie } => {
            let avatars = api::fetch_avatars(client, &cookie, &user_ids).await?;
            Ok(BackendEvent::AvatarsUpdated(avatars))
        }
        BackendCommand::RefreshPresence { user_ids, cookie } => {
            let presences = api::fetch_presences(client, &cookie, &user_ids).await?;
            Ok(BackendEvent::PresencesUpdated(presences))
        }
        BackendCommand::LaunchGameEncrypted {
            user_id,
            encrypted_cookie,
            password,
            use_credential_manager,
            place_id,
            job_id,
            link_code,
            access_code,
            multi_instance,
            kill_background,
            privacy_mode,
        } => {
            let cookie = if use_credential_manager {
                crypto::credential_load(user_id)?
            } else {
                let enc = encrypted_cookie.ok_or_else(|| {
                    CoreError::Crypto("no encrypted cookie stored for this account".into())
                })?;
                crypto::decrypt_cookie(&enc, &password)?
            };
            if multi_instance {
                process::enable_multi_instance()?;
            }
            if kill_background || multi_instance {
                process::kill_tray_roblox();
            }
            if privacy_mode {
                process::clear_roblox_cookies();
            }
            let ticket = client.generate_auth_ticket(&cookie).await?;
            process::launch_game(&ticket, place_id, job_id.as_deref(), link_code.as_deref(), access_code.as_deref())?;
            Ok(BackendEvent::GameLaunched)
        }
        BackendCommand::LaunchGame {
            cookie,
            place_id,
            job_id,
            link_code,
            access_code,
            multi_instance,
            kill_background,
            privacy_mode,
        } => {
            if multi_instance {
                process::enable_multi_instance()?;
            }
            if kill_background || multi_instance {
                process::kill_tray_roblox();
            }
            if privacy_mode {
                process::clear_roblox_cookies();
            }
            let ticket = client.generate_auth_ticket(&cookie).await?;
            process::launch_game(&ticket, place_id, job_id.as_deref(), link_code.as_deref(), access_code.as_deref())?;
            Ok(BackendEvent::GameLaunched)
        }
        BackendCommand::SaveStore {
            store,
            path,
            password,
        } => {
            crypto::save_encrypted(&path, &store, &password)?;
            Ok(BackendEvent::StoreSaved)
        }
        BackendCommand::LoadStore { path, password } => {
            let store = crypto::load_encrypted(&path, &password)?;
            Ok(BackendEvent::StoreLoaded(store))
        }
        BackendCommand::KillAll => {
            let count = process::kill_all_roblox()?;
            Ok(BackendEvent::Killed(count))
        }
        BackendCommand::RefreshAll {
            user_ids,
            first_user_id,
            encrypted_cookie,
            password,
            use_credential_manager,
        } => {
            let cookie = if use_credential_manager {
                crypto::credential_load(first_user_id)?
            } else {
                let enc = encrypted_cookie.ok_or_else(|| {
                    CoreError::Crypto("no encrypted cookie for refresh".into())
                })?;
                crypto::decrypt_cookie(&enc, &password)?
            };
            let avatars = api::fetch_avatars(client, &cookie, &user_ids).await?;
            let _ = tx.send(BackendEvent::AvatarsUpdated(avatars.clone()));
            // Download actual image bytes (skips failures)
            let images = api::download_avatar_images(client, &cookie, &avatars).await;
            if !images.is_empty() {
                let _ = tx.send(BackendEvent::AvatarImagesReady(images));
            }
            let presences = api::fetch_presences(client, &cookie, &user_ids).await?;
            Ok(BackendEvent::PresencesUpdated(presences))
        }
        BackendCommand::RefreshPresenceOnly {
            user_ids,
            first_user_id,
            encrypted_cookie,
            password,
            use_credential_manager,
        } => {
            let cookie = if use_credential_manager {
                crypto::credential_load(first_user_id)?
            } else {
                let enc = encrypted_cookie.ok_or_else(|| {
                    CoreError::Crypto("no encrypted cookie for refresh".into())
                })?;
                crypto::decrypt_cookie(&enc, &password)?
            };
            let presences = api::fetch_presences(client, &cookie, &user_ids).await?;
            Ok(BackendEvent::PresencesUpdated(presences))
        }
        BackendCommand::BulkLaunchEncrypted {
            accounts,
            password,
            use_credential_manager,
            place_id,
            job_id,
            link_code,
            access_code,
            multi_instance,
            kill_background,
            privacy_mode,
        } => {
            if multi_instance {
                process::enable_multi_instance()?;
            }
            if kill_background || multi_instance {
                process::kill_tray_roblox();
            }
            if privacy_mode {
                process::clear_roblox_cookies();
            }

            // If no Job ID was provided and no link_code (private server), resolve
            // a public server so all accounts land in the same server.
            let resolved_job_id = if job_id.is_some() || link_code.is_some() {
                job_id
            } else {
                // Decrypt the first account's cookie to make the API call
                let first = accounts.first().ok_or_else(|| {
                    CoreError::Process("no accounts to launch".into())
                })?;
                let first_cookie = if use_credential_manager {
                    crypto::credential_load(first.0)?
                } else {
                    match &first.1 {
                        Some(enc) => crypto::decrypt_cookie(enc, &password)?,
                        None => {
                            return Err(CoreError::Crypto(
                                "no encrypted cookie for first account".into(),
                            ))
                        }
                    }
                };
                match api::fetch_servers(client, &first_cookie, place_id, None).await {
                    Ok((servers, _)) => {
                        if let Some(server) = servers.into_iter().next() {
                            info!("Bulk launch: resolved server {} ({}/{} players)",
                                  server.id, server.playing, server.max_players);
                            Some(server.id)
                        } else {
                            info!("Bulk launch: no public servers found, launching without Job ID");
                            None
                        }
                    }
                    Err(e) => {
                        info!("Bulk launch: server fetch failed ({e}), launching without Job ID");
                        None
                    }
                }
            };

            let total = accounts.len();
            let mut launched = 0usize;
            let mut failed = 0usize;
            for (i, (user_id, encrypted_cookie)) in accounts.iter().enumerate() {
                let cookie_result = if use_credential_manager {
                    crypto::credential_load(*user_id)
                } else {
                    match encrypted_cookie {
                        Some(enc) => crypto::decrypt_cookie(enc, &password),
                        None => Err(CoreError::Crypto(
                            "no encrypted cookie stored".into(),
                        )),
                    }
                };
                match cookie_result {
                    Ok(cookie) => {
                        match client.generate_auth_ticket(&cookie).await {
                            Ok(ticket) => {
                                if let Err(e) = process::launch_game(
                                    &ticket,
                                    place_id,
                                    resolved_job_id.as_deref(),
                                    link_code.as_deref(),
                                    access_code.as_deref(),
                                ) {
                                    error!("Bulk launch failed for user {user_id}: {e}");
                                    failed += 1;
                                } else {
                                    launched += 1;
                                }
                            }
                            Err(e) => {
                                error!("Auth ticket failed for user {user_id}: {e}");
                                failed += 1;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Cookie decrypt failed for user {user_id}: {e}");
                        failed += 1;
                    }
                }
                let _ = tx.send(BackendEvent::BulkLaunchProgress {
                    launched: i + 1,
                    total,
                });
                // Kill tray processes that spawn between launches
                if (kill_background || multi_instance) && i + 1 < total {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    process::kill_tray_roblox();
                }
            }
            Ok(BackendEvent::BulkLaunchComplete { launched, failed })
        }
        BackendCommand::RevalidateAll {
            accounts,
            password,
            use_credential_manager,
        } => {
            for (user_id, encrypted_cookie) in &accounts {
                let cookie_result = if use_credential_manager {
                    crypto::credential_load(*user_id)
                } else {
                    match encrypted_cookie {
                        Some(enc) => crypto::decrypt_cookie(enc, &password),
                        None => continue,
                    }
                };
                let cookie = match cookie_result {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                match client.validate_cookie(&cookie).await {
                    Ok((_, username, display_name)) => {
                        let _ = tx.send(BackendEvent::AccountRevalidated {
                            user_id: *user_id,
                            valid: true,
                            username,
                            display_name,
                        });
                    }
                    Err(_) => {
                        info!("Cookie expired for user {user_id}");
                        let _ = tx.send(BackendEvent::AccountRevalidated {
                            user_id: *user_id,
                            valid: false,
                            username: String::new(),
                            display_name: String::new(),
                        });
                    }
                }
            }
            Ok(BackendEvent::StoreSaved)
        }
        BackendCommand::ArrangeWindows => {
            // Small delay to let Roblox windows finish appearing
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            process::arrange_roblox_windows();
            Ok(BackendEvent::WindowsArranged)
        }
        BackendCommand::CheckForUpdates { current_version } => {
            match api::check_for_updates(&current_version).await {
                Ok(Some((version, url))) => {
                    Ok(BackendEvent::UpdateAvailable { version, url })
                }
                Ok(None) => Ok(BackendEvent::StoreSaved), // no-op event
                Err(e) => {
                    info!("Update check failed (non-fatal): {e}");
                    Ok(BackendEvent::StoreSaved) // silently ignore
                }
            }
        }
        BackendCommand::ResolvePlace { place_id, universe_id, index } => {
            // Both the game name and icon endpoints work without auth when we
            // have a universe_id. If we don't, we can't resolve without auth.
            if let Some(uid) = universe_id {
                let name = api::resolve_universe_name(client, uid).await
                    .unwrap_or_default();
                let icon_bytes = match api::fetch_game_icons(client, "", &[uid]).await {
                    Ok(icons) => {
                        if let Some((_, url)) = icons.into_iter().next() {
                            client.get_bytes(&url, "").await.ok()
                        } else {
                            None
                        }
                    }
                    Err(e) => {
                        info!("Game icon fetch failed for universe {uid}: {e}");
                        None
                    }
                };
                Ok(BackendEvent::PlaceResolved { index, place_name: name, place_id, icon_bytes })
            } else {
                // No universe_id — cannot resolve without auth. Return empty.
                Ok(BackendEvent::PlaceResolved { index, place_name: String::new(), place_id, icon_bytes: None })
            }
        }
        BackendCommand::ResolveShareLink {
            share_code,
            server_name,
            first_user_id,
            encrypted_cookie,
            password,
            use_credential_manager,
        } => {
            let cookie = if use_credential_manager {
                crypto::credential_load(first_user_id)?
            } else {
                let enc = encrypted_cookie.ok_or_else(|| {
                    CoreError::Crypto("no encrypted cookie for share link resolution".into())
                })?;
                crypto::decrypt_cookie(&enc, &password)?
            };
            match api::resolve_share_link(client, &cookie, &share_code).await {
                Ok((place_id, universe_id, link_code, access_code)) => {
                    Ok(BackendEvent::ShareLinkResolved {
                        server_name,
                        place_id,
                        universe_id,
                        link_code,
                        access_code,
                    })
                }
                Err(e) => {
                    info!("ResolveShareLink failed: {e}");
                    Ok(BackendEvent::ShareLinkFailed(e.to_string()))
                }
            }
        }
    }
}
