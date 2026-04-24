//! Subprocess-based browser login to Roblox.
//!
//! `wry` + `tao` don't coexist with `eframe`'s main-thread `winit` event loop
//! in the same process — even with `EventLoopBuilderExtWindows::with_any_thread`,
//! `WindowBuilder::build` dies with a native Win32 exception when there's
//! already a winit-owned window pump elsewhere in the process. Workaround:
//! re-exec our own binary with a hidden flag so the child has a genuine main
//! thread to host the webview on. The child writes the captured cookie to a
//! file we hand it, then exits; the parent waits on the child and reads that
//! file to produce a [`LoginOutcome`].
//!
//! The wire format is trivially simple: the outfile only exists on success and
//! contains the raw `.ROBLOSECURITY` value. Anything else (missing file, empty
//! file, child exit != 0) is treated as cancel/failure.
//!
//! `main()` dispatches the child mode via [`FLAG`] before `eframe::run_native`
//! so the normal UI never initializes in the child process.

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use tracing::{info, warn};
use wry::{WebContext, WebViewBuilder};

/// CLI flag that switches `main()` into child webview mode.
/// Invoked as: `ram_ui.exe --browser-login <profile_dir> <outfile>`.
pub const FLAG: &str = "--browser-login";

const LOGIN_URL: &str = "https://www.roblox.com/login";
const POLL_INTERVAL: Duration = Duration::from_millis(400);

pub enum LoginOutcome {
    Success(String),
    Cancelled,
    Failed(String),
}

// ---------------------------------------------------------------------------
// Parent side — spawn the helper subprocess and deliver its result.
// ---------------------------------------------------------------------------

pub fn spawn(profile_dir: PathBuf, tx: Sender<LoginOutcome>) {
    std::thread::spawn(move || {
        let outcome = match spawn_and_wait(profile_dir) {
            Ok(o) => o,
            Err(e) => {
                warn!("browser_login parent: {e}");
                LoginOutcome::Failed(e)
            }
        };
        let _ = tx.send(outcome);
    });
}

fn spawn_and_wait(profile_dir: PathBuf) -> Result<LoginOutcome, String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let outfile = profile_dir.join("cookie.out");
    // Clear any leftover from a prior attempt so `exists()` means "this run"
    let _ = std::fs::remove_file(&outfile);

    info!("browser_login parent: spawning child {}", exe.display());
    let status = std::process::Command::new(&exe)
        .arg(FLAG)
        .arg(&profile_dir)
        .arg(&outfile)
        .status()
        .map_err(|e| format!("spawn child: {e}"))?;

    if !status.success() {
        return Err(format!("child exited unsuccessfully: {status}"));
    }

    match std::fs::read_to_string(&outfile) {
        Ok(cookie) if !cookie.trim().is_empty() => {
            let cookie = cookie.trim().to_string();
            let _ = std::fs::remove_file(&outfile);
            Ok(LoginOutcome::Success(cookie))
        }
        _ => Ok(LoginOutcome::Cancelled),
    }
}

// ---------------------------------------------------------------------------
// Child side — runs on this process's main thread, hosts the webview, exits.
// ---------------------------------------------------------------------------

/// Entry point for the child process. Blocks until the user logs in or closes
/// the window, then returns an exit code for `std::process::exit`.
pub fn run_child(profile_dir: PathBuf, outfile: PathBuf) -> i32 {
    match run_child_inner(profile_dir, outfile) {
        Ok(()) => 0,
        Err(e) => {
            warn!("browser_login child: {e}");
            1
        }
    }
}

fn run_child_inner(profile_dir: PathBuf, outfile: PathBuf) -> Result<(), String> {
    info!("browser_login child: start, profile={}, out={}", profile_dir.display(), outfile.display());
    std::fs::create_dir_all(&profile_dir)
        .map_err(|e| format!("create profile dir: {e}"))?;

    let event_loop = EventLoopBuilder::<()>::new().build();

    let window = WindowBuilder::new()
        .with_title("Log in to Roblox")
        .with_inner_size(tao::dpi::LogicalSize::new(500.0, 720.0))
        .build(&event_loop)
        .map_err(|e| format!("window build: {e}"))?;

    let mut web_context = WebContext::new(Some(profile_dir));

    let webview = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_url(LOGIN_URL)
        .build(&window)
        .map_err(|e| format!("webview build: {e}"))?;

    info!("browser_login child: entering event loop");
    let mut next_poll = Instant::now() + POLL_INTERVAL;
    let mut done = false;

    // `run` diverges (`-> !`) — the process exits when we request
    // ControlFlow::Exit, so there's no code after this point.
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(next_poll);

        match event {
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if !done {
                    if let Some(cookie) = try_extract_cookie(&webview) {
                        info!("browser_login child: cookie captured, writing outfile");
                        if let Err(e) = std::fs::write(&outfile, &cookie) {
                            warn!("failed to write cookie outfile: {e}");
                        }
                        done = true;
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                }
                next_poll = Instant::now() + POLL_INTERVAL;
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    })
}

fn try_extract_cookie(webview: &wry::WebView) -> Option<String> {
    let cookies = webview.cookies().ok()?;
    cookies.into_iter().find_map(|c| {
        if c.name() == ".ROBLOSECURITY" && !c.value().is_empty() {
            Some(c.value().to_string())
        } else {
            None
        }
    })
}
