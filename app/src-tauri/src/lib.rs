//! The shell: windows, tray, the tick, and the OS edges.
//!
//! Everything that decides *what* happens lives in `threshold-core`. This file
//! decides *where* it happens — which monitor, which window, whether input is
//! allowed through — and makes the noise.

pub mod audio;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use chrono::{DateTime, Local};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, Wry,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as AutostartExt};
use tauri_plugin_notification::NotificationExt;
use threshold_core::{
    should_fire, Anchors, Config, Cue, CueDraft, Fire, Font, Layout, Repeat, Schedule, Store,
    WrapSpec,
};

static FONT_BYTES: &[u8] = include_bytes!("../../../core/assets/texgyreadventor-bold.otf");

/// Where a crash report goes. One place, so the tray item and the panic hook
/// cannot disagree about it.
const REPO_URL: &str = "https://github.com/mconcepcionb/threshold";

/// One line per window-level event. A visual artefact is either caused by one of
/// these or it is not, and the log says which.
fn trace(what: &str) {
    eprintln!("threshold: {} {what}", Local::now().format("%H:%M:%S%.3f"));
}

fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Alpha software should not die silently. A panic writes the report next to the
/// config and opens a prefilled issue: the person sees what happened, and the
/// report is one click from arriving somewhere useful.
fn install_crash_reporter(dir: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = format!(
            "threshold {}\n{}\n{}\n\n{}",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            Local::now().to_rfc3339(),
            info
        );
        let path = dir.join("crash.log");
        let _ = std::fs::write(&path, &report);
        eprintln!("threshold: crashed, wrote {}", path.display());

        let trimmed: String = report.chars().take(1_200).collect();
        let url = format!(
            "{REPO_URL}/issues/new?title={}&body={}",
            percent_encode("Crash"),
            percent_encode(&trimmed)
        );
        open_url(&url);

        previous(info);
    }));
}

struct AutostartItem(Mutex<Option<CheckMenuItem<Wry>>>);
struct MuteItem(Mutex<Option<CheckMenuItem<Wry>>>);

/// One transparent layer per monitor, created at that monitor's size and never
/// resized. Resizing a webview makes it reflow and stretch while it happens,
/// which is the "scale" seen on the first card — and a window born the right
/// size never has to do it.
#[derive(Clone)]
struct Overlay {
    label: String,
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    scale: f64,
}

struct Runtime {
    store: Store,
    /// Where interval anchors live: beside the config, never inside it.
    anchors: Anchors,
    state_path: PathBuf,
    overlays: Vec<Overlay>,
    /// Sound off, cards still fire. The polite half of an emergency stop.
    muted: bool,
    /// No cards at all until this instant. The impolite half.
    paused_until: Option<DateTime<Local>>,
    /// The layer currently showing a card, if any, and the window that had focus
    /// before it took over — so the keyboard can be handed back.
    active: Option<String>,
    focus_back: Option<isize>,
    /// The next moment each cue is due. Rebuilt on reload, advanced on fire.
    next: HashMap<u64, Option<DateTime<Local>>>,
    font: Arc<Font>,
    /// A config error we survived: the app is running on the last good config
    /// (or a fresh one) and the settings window should say so.
    trouble: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StateDto {
    config_path: String,
    changed_on_disk: bool,
    floor_ms: u32,
    volume: f32,
    cues: Vec<CueDraft>,
    /// When each cue is next due, RFC 3339, aligned with `cues`.
    next: Vec<Option<String>>,
    trouble: Option<String>,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CardPayload {
    lines: Vec<String>,
    size_px: f32,
    seconds: u32,
    floor_ms: u32,
}

// ---------------------------------------------------------------------------
// commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_state(app: AppHandle) -> Result<StateDto, String> {
    // The settings window is created from the config file and asks for this the
    // moment its script runs, which can be before `setup` has managed the state.
    // Answering "starting" lets the page retry instead of dying on a hard error.
    let state = app.try_state::<Mutex<Runtime>>().ok_or("starting")?;
    let rt = state.lock().map_err(|_| "state poisoned".to_string())?;
    let now = Local::now();
    let scheduled = rt.cues();
    Ok(StateDto {
        config_path: rt.store.path().display().to_string(),
        changed_on_disk: rt.store.changed_on_disk(),
        floor_ms: rt.store.config().floor_ms,
        volume: rt.store.config().volume,
        cues: rt.store.config().drafts(),
        next: scheduled
            .iter()
            .map(|c| {
                if c.enabled {
                    c.next_fire(now).map(|at| at.to_rfc3339())
                } else {
                    None
                }
            })
            .collect(),
        trouble: rt.trouble.clone(),
    })
}

#[tauri::command]
fn save_state(
    app: AppHandle,
    rt: State<'_, Mutex<Runtime>>,
    floor_ms: u32,
    volume: f32,
    cues: Vec<CueDraft>,
) -> Result<(), String> {
    let mut rt = rt.lock().map_err(|_| "state poisoned".to_string())?;
    let mut config = rt.store.config().clone();
    config.floor_ms = floor_ms;
    config.volume = volume.clamp(0.0, 1.0);
    config.from_drafts(&cues).map_err(|e| e.to_string())?;
    rt.store.save(config).map_err(|e| e.to_string())?;
    rt.trouble = None;
    rebuild_next(&mut rt);
    refresh_tray(&app, &rt);
    Ok(())
}

#[tauri::command]
fn reload_config(app: AppHandle, rt: State<'_, Mutex<Runtime>>) -> Result<(), String> {
    let mut rt = rt.lock().map_err(|_| "state poisoned".to_string())?;
    rt.store.reload().map_err(|e| e.to_string())?;
    rt.trouble = None;
    rebuild_next(&mut rt);
    refresh_tray(&app, &rt);
    Ok(())
}

#[tauri::command]
fn fire_preview(app: AppHandle, rt: State<'_, Mutex<Runtime>>) -> Result<(), String> {
    let (font, cue, floor) = {
        let rt = rt.lock().map_err(|_| "state poisoned".to_string())?;
        let cue = rt
            .store
            .config()
            .cues
            .first()
            .cloned()
            .unwrap_or_else(|| Cue::at("THRESHOLD", chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap()));
        (rt.font.clone(), cue, rt.store.config().floor_ms)
    };
    fire_card(&app, &font, &cue, floor);
    Ok(())
}

/// Fire one cue exactly as it is being edited, without saving it. The settings
/// list is the only place a card can be judged without waiting for its time.
#[tauri::command]
fn preview_cue(app: AppHandle, rt: State<'_, Mutex<Runtime>>, cue: CueDraft) -> Result<(), String> {
    let (font, floor, cue) = {
        let rt = rt.lock().map_err(|_| "state poisoned".to_string())?;
        let cue = cue.to_cue(0).map_err(|e| e)?;
        (rt.font.clone(), rt.store.config().floor_ms, cue)
    };
    fire_card(&app, &font, &cue, floor);
    Ok(())
}

#[tauri::command]
fn add_examples(app: AppHandle, rt: State<'_, Mutex<Runtime>>) -> Result<usize, String> {
    let mut rt = rt.lock().map_err(|_| "state poisoned".to_string())?;
    let mut config = rt.store.config().clone();
    let added = config.merge_starter();
    if added > 0 {
        rt.store.save(config).map_err(|e| e.to_string())?;
        rebuild_next(&mut rt);
        refresh_tray(&app, &rt);
    }
    Ok(added)
}

/// The Floor is over: input now lands on the card, and it takes the keyboard.
#[tauri::command]
fn card_floor(app: AppHandle, label: String) {
    let state = app.state::<Mutex<Runtime>>();
    let active = {
        let rt = match state.lock() {
            Ok(rt) => rt,
            Err(_) => return,
        };
        rt.active.clone()
    };
    if active.as_deref() != Some(label.as_str()) {
        return;
    }
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.set_ignore_cursor_events(false);
        let _ = w.set_focus();
    }
    trace(&format!("floor {label}"));
}

/// The Window is over, or the card was dismissed early: input goes back to
/// pass-through and the keyboard goes back to whoever had it.
#[tauri::command]
fn card_end(app: AppHandle, label: String) {
    let state = app.state::<Mutex<Runtime>>();
    let previous = {
        let mut rt = match state.lock() {
            Ok(rt) => rt,
            Err(_) => return,
        };
        if rt.active.as_deref() != Some(label.as_str()) {
            return;
        }
        rt.active = None;
        rt.focus_back.take()
    };
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.set_ignore_cursor_events(true);
    }
    if let Some(handle) = previous {
        focus_window(handle);
    }
    trace(&format!("end {label}"));
}

#[tauri::command]
fn open_settings(app: AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

// ---------------------------------------------------------------------------
// firing
// ---------------------------------------------------------------------------

/// A locked screen cannot show a window. The card still fires and the sting still
/// plays — but the only thing that reaches the person is a notification.
#[cfg(windows)]
fn screen_is_locked() -> bool {
    use windows::Win32::System::StationsAndDesktops::{
        CloseDesktop, OpenInputDesktop, DESKTOP_READOBJECTS,
    };

    // The input desktop is Winlogon when the workstation is locked, and a session
    // that is not on it cannot open it.
    match unsafe { OpenInputDesktop(Default::default(), false, DESKTOP_READOBJECTS) } {
        Ok(desktop) => {
            let _ = unsafe { CloseDesktop(desktop) };
            false
        }
        Err(_) => true,
    }
}

#[cfg(not(windows))]
fn screen_is_locked() -> bool {
    false
}

fn notify_behind_the_lock(app: &AppHandle, cue: &Cue) {
    if let Err(e) = app
        .notification()
        .builder()
        .title("Threshold")
        .body(&cue.text)
        .show()
    {
        eprintln!("threshold: could not post a notification: {e}");
    }
}

/// Which window had focus before a card took it. Taking focus is part of the
/// interruption; giving it back is not optional, because the layer never hides.
#[cfg(windows)]
fn foreground_window() -> Option<isize> {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.0.is_null()).then_some(hwnd.0 as isize)
}

#[cfg(windows)]
fn focus_window(handle: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
    unsafe {
        let _ = SetForegroundWindow(HWND(handle as *mut _));
    }
}

#[cfg(not(windows))]
fn foreground_window() -> Option<isize> {
    None
}

#[cfg(not(windows))]
fn focus_window(_handle: isize) {}

fn fire_card(app: &AppHandle, font: &Font, cue: &Cue, floor_ms: u32) {
    // Which layer is under the pointer right now. Each one was born at its own
    // monitor's size, so nothing here resizes anything.
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());

    let (overlays, muted, volume) = {
        let state = app.state::<Mutex<Runtime>>();
        let mut rt = match state.lock() {
            Ok(rt) => rt,
            Err(_) => return,
        };
        rt.active = Some(String::new()); // filled once the monitor is known
        rt.focus_back = foreground_window();
        (rt.overlays.clone(), rt.muted, rt.store.config().volume)
    };

    let target = monitor
        .as_ref()
        .and_then(|m| {
            overlays
                .iter()
                .find(|o| o.position.x == m.position().x && o.position.y == m.position().y)
                .cloned()
        })
        .or_else(|| overlays.first().cloned());

    let Some(target) = target else { return };
    if app.get_webview_window(&target.label).is_none() {
        return;
    }

    let width_css = target.size.width as f32 / target.scale as f32;
    let Layout { lines, size_px } =
        threshold_core::layout::wrap(font, &cue.text.to_uppercase(), width_css, &WrapSpec::hd());

    let payload = CardPayload { lines, size_px, seconds: cue.seconds, floor_ms };

    // A locked screen cannot show a window, so a card fired now would be spent
    // unseen. The sting still plays; the notification is what reaches them.
    let locked = screen_is_locked();
    trace(&format!("fire \"{}\" locked={locked}", cue.text));

    // one card at a time: every layer is always up, so the other one is only
    // cleared by the page when it dismisses
    if !locked {
        let _ = app.emit_to(&target.label, "card", payload);
        let state = app.state::<Mutex<Runtime>>();
        let mut rt = match state.lock() {
            Ok(rt) => rt,
            Err(_) => return,
        };
        rt.active = Some(target.label.clone());
        drop(rt);
    }

    if !muted {
        audio::fire(cue.sting, volume);
    }

    if locked {
        notify_behind_the_lock(app, cue);
        return;
    }

    // The page owns the timing: it calls `card_floor` at the Floor and `card_end`
    // when the Window is over. Rust only moves input and focus, so there is one
    // clock instead of two that can disagree.
}

fn rebuild_next(rt: &mut Runtime) {
    let now = Local::now();
    rt.next = rt.cues().iter().map(|c| (c.id, c.next_fire(now))).collect();
}

impl Runtime {
    /// The cue list as the scheduler should see it: the config on disk, with the
    /// sidecar anchors applied. The config file itself is never rewritten for them.
    fn cues(&self) -> Vec<Cue> {
        let mut cues = self.store.config().cues.clone();
        self.anchors.apply(&mut cues);
        cues
    }
}

// ---------------------------------------------------------------------------
// the tick
// ---------------------------------------------------------------------------

fn tick(app: &AppHandle) {
    let state = app.state::<Mutex<Runtime>>();
    let (to_fire, font, floor_ms) = {
        let mut rt = match state.lock() {
            Ok(rt) => rt,
            Err(_) => return,
        };
        let now = Local::now();
        let cues = rt.cues();
        let mut to_fire: Vec<Cue> = Vec::new();
        let mut moved: Vec<(Cue, DateTime<Local>)> = Vec::new();
        let mut spent: Vec<u64> = Vec::new();

        for cue in cues.iter().filter(|c| c.enabled) {
            let Some(due) = rt.next.get(&cue.id).copied().flatten() else { continue };
            if due > now {
                continue;
            }

            if !matches!(should_fire(cue.late, due, now), Fire::Skip) {
                to_fire.push(cue.clone());
                if matches!(cue.schedule, Schedule::Every(_)) {
                    moved.push((cue.clone(), due));
                }
                // a one-shot that fired is spent: it should not sit there armed
                // forever, showing "missed" the next morning
                if matches!(cue.repeat, Repeat::Once) {
                    spent.push(cue.id);
                }
            }
            rt.next.insert(cue.id, cue.next_fire(now));
        }

        // An interval chain is anchored to when it was due, not to when the tick
        // noticed, so a slow tick cannot drift the phase. The anchor goes to the
        // sidecar: writing the config here would rewrite a file a human edits.
        if !moved.is_empty() {
            for (cue, due) in moved {
                rt.anchors.set(&cue, due);
            }
            if let Err(e) = rt.anchors.save(&rt.state_path) {
                eprintln!("threshold: could not persist the interval anchors: {e}");
            }
        }

        if !spent.is_empty() {
            let mut config = rt.store.config().clone();
            for id in &spent {
                if let Some(c) = config.cues.iter_mut().find(|c| c.id == *id) {
                    c.enabled = false;
                }
            }
            if let Err(e) = rt.store.save(config) {
                eprintln!("threshold: could not disarm spent cues: {e}");
            }
        }

        (to_fire, rt.font.clone(), rt.store.config().floor_ms)
    };

    for cue in to_fire {
        fire_card(app, &font, &cue, floor_ms);
    }
}

// ---------------------------------------------------------------------------
// tray
// ---------------------------------------------------------------------------

fn refresh_tray(app: &AppHandle, rt: &Runtime) {    let Some(tray) = app.tray_by_id("main") else { return };
    let now = Local::now();
    let next = rt
        .store
        .config()
        .cues
        .iter()
        .filter(|c| c.enabled)
        .filter_map(|c| c.next_fire(now).map(|at| (at, &c.text)))
        .min_by_key(|(at, _)| *at);

    let tooltip = match next {
        Some((at, text)) => {
            let mins = (at - now).num_minutes().max(0);
            format!("Threshold · {text} in {}h{:02}m", mins / 60, mins % 60)
        }
        None => "Threshold · nothing armed".to_string(),
    };
    let _ = tray.set_tooltip(Some(tooltip));
}

fn fire_first(app: &AppHandle) {
    let state = app.state::<Mutex<Runtime>>();
    let (font, cue, floor) = {
        let rt = match state.lock() {
            Ok(rt) => rt,
            Err(_) => return,
        };
        match rt.store.config().cues.first().cloned() {
            Some(cue) => (rt.font.clone(), cue, rt.store.config().floor_ms),
            None => return,
        }
    };
    fire_card(app, &font, &cue, floor);
}

fn pause_for_an_hour(app: &AppHandle) {
    let state = app.state::<Mutex<Runtime>>();
    let mut rt = match state.lock() {
        Ok(rt) => rt,
        Err(_) => return,
    };
    rt.paused_until = Some(Local::now() + chrono::Duration::hours(1));
    refresh_tray(app, &rt);
}

fn resume_now(app: &AppHandle) {
    let state = app.state::<Mutex<Runtime>>();
    let mut rt = match state.lock() {
        Ok(rt) => rt,
        Err(_) => return,
    };
    rt.paused_until = None;
    rebuild_next(&mut rt);
    refresh_tray(app, &rt);
}

fn toggle_mute(app: &AppHandle) {
    let state = app.state::<Mutex<Runtime>>();
    let mut rt = match state.lock() {
        Ok(rt) => rt,
        Err(_) => return,
    };
    rt.muted = !rt.muted;
    let muted = rt.muted;
    drop(rt);
    if let Some(item) = app.state::<MuteItem>().0.lock().ok().and_then(|g| g.clone()) {
        let _ = item.set_checked(muted);
    }
}

fn toggle_autostart(app: &AppHandle) {
    let state = app.state::<AutostartItem>();
    let guard = match state.0.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let Some(item) = guard.as_ref() else { return };

    // Ask the OS rather than the checkbox: the item's own checked state is a
    // mirror, and a mirror can drift. Enabling is idempotent, so toggling from
    // the real state is always correct.
    let manager = app.autolaunch();
    let currently = manager.is_enabled().unwrap_or(false);
    let want = !currently;
    let result = if want { manager.enable() } else { manager.disable() };

    match result {
        Ok(()) => {
            let _ = item.set_checked(want);
        }
        Err(e) => {
            eprintln!("threshold: could not change start-at-login: {e}");
            let _ = item.set_checked(currently);
        }
    }
}

fn reload_runtime(app: &AppHandle) {
    let state = app.state::<Mutex<Runtime>>();
    let mut rt = match state.lock() {
        Ok(rt) => rt,
        Err(_) => return,
    };
    if let Err(e) = rt.store.reload() {
        eprintln!("threshold: config reload failed: {e}");
        return;
    }
    rt.trouble = None;
    rebuild_next(&mut rt);
    refresh_tray(app, &rt);
}

// ---------------------------------------------------------------------------
// boot
// ---------------------------------------------------------------------------

fn config_path() -> PathBuf {
    directories::ProjectDirs::from("dev", "threshold", "threshold")
        .map(|d| d.config_dir().join("cues.toml"))
        .unwrap_or_else(|| PathBuf::from("cues.toml"))
}

/// A broken config must not stop the app from starting. The bad file is kept
/// next to the new one, and the settings window explains what happened.
fn open_store(path: &PathBuf) -> (Store, Option<String>) {
    // a new install arrives with the starter set and one card due in 90 seconds
    match Store::open_with_seed(path, Config::starter_at(Local::now())) {
        Ok(store) => (store, None),
        Err(e) => {
            let backup = path.with_extension("toml.broken");
            let _ = std::fs::rename(path, &backup);
            let store = Store::open_with_seed(path, Config::starter_at(Local::now()))
                .expect("a fresh config can always be written");
            (
                store,
                Some(format!(
                    "{e}\nThe old file was kept as {}.",
                    backup.display()
                )),
            )
        }
    }
}

pub fn run() {
    tauri::Builder::default()
        // Must be the first plugin. A second launch raises the first window
        // instead of starting a second scheduler: two would thump twice.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("settings") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let path = config_path();
            install_crash_reporter(
                path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf(),
            );
            let (store, trouble) = open_store(&path);
            let font = Font::from_bytes(FONT_BYTES.to_vec())
                .map_err(|e| format!("bundled font is unreadable: {e}"))?;

            // anchors live beside the config, keyed by what the cue is
            let state_path = path.with_file_name("state.toml");
            let anchors = Anchors::load(&state_path);

            // Manage the state before anything slow happens. The settings window
            // is created from the config file and starts asking for it
            // immediately; building the overlay webviews first made that request
            // arrive before the state existed.
            let mut runtime = Runtime {
                store,
                anchors,
                state_path,
                overlays: Vec::new(),
                muted: false,
                paused_until: None,
                active: None,
                focus_back: None,
                next: HashMap::new(),
                font: Arc::new(font),
                trouble,
            };
            rebuild_next(&mut runtime);
            app.manage(Mutex::new(runtime));

            // One transparent layer per monitor, born at that monitor's size.
            // Nothing is ever resized, so nothing can reflow or stretch.
            let mut overlays = Vec::new();
            let monitors = app.available_monitors().unwrap_or_default();
            for (i, monitor) in monitors.iter().enumerate() {
                let label = format!("overlay-{i}");
                let scale = monitor.scale_factor();
                let logical_w = monitor.size().width as f64 / scale;
                let logical_h = monitor.size().height as f64 / scale;

                let window = tauri::WebviewWindowBuilder::new(
                    app,
                    &label,
                    tauri::WebviewUrl::App("index.html?window=overlay".into()),
                )
                .title("Threshold card")
                .decorations(false)
                .transparent(true)
                .background_color(tauri::window::Color(0, 0, 0, 0))
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .shadow(false)
                .visible(true)
                .focused(false)
                .position(monitor.position().x as f64 / scale, monitor.position().y as f64 / scale)
                .inner_size(logical_w, logical_h)
                .build()?;

                // idle: the layer is scenery and must not intercept a single click
                let _ = window.set_ignore_cursor_events(true);
                eprintln!(
                    "threshold: {label} born at {:?} size {:?} scale {scale}",
                    monitor.position(),
                    monitor.size()
                );

                overlays.push(Overlay {
                    label,
                    position: *monitor.position(),
                    size: *monitor.size(),
                    scale,
                });
            }

            // the layers were built above; hand them to the scheduler
            {
                let state = app.state::<Mutex<Runtime>>();
                let mut rt = state.lock().expect("state was just managed");
                rt.overlays = overlays.clone();
            }

            // The layers are never shown or hidden. A webview that is hidden and
            // resumed repaints blank on the frame it comes back, which is the white
            // blink: the card is rendered, the surface is not. Leaving every layer
            // up and click-through means a card is only a DOM change.


            let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
            let autostart = CheckMenuItem::with_id(
                app,
                "autostart",
                "Start at login",
                true,
                autostart_on,
                None::<&str>,
            )?;
            app.manage(AutostartItem(Mutex::new(Some(autostart.clone()))));

            let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let fire = MenuItem::with_id(app, "fire", "Fire a test card", true, None::<&str>)?;
            let reload = MenuItem::with_id(app, "reload", "Reload config", true, None::<&str>)?;
            let pause = MenuItem::with_id(app, "pause", "Pause for an hour", true, None::<&str>)?;
            let resume = MenuItem::with_id(app, "resume", "Resume", true, None::<&str>)?;
            let mute = CheckMenuItem::with_id(app, "mute", "Mute the sting", true, false, None::<&str>)?;
            app.manage(MuteItem(Mutex::new(Some(mute.clone()))));
            let report = MenuItem::with_id(app, "report", "Report a problem", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &settings,
                    &fire,
                    &reload,
                    &PredefinedMenuItem::separator(app)?,
                    &pause,
                    &resume,
                    &mute,
                    &PredefinedMenuItem::separator(app)?,
                    &autostart,
                    &PredefinedMenuItem::separator(app)?,
                    &report,
                    &quit,
                ],
            )?;

            let mut tray = TrayIconBuilder::with_id("main")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => open_settings(app.clone()),
                    "fire" => fire_first(app),
                    "reload" => reload_runtime(app),
                    "pause" => pause_for_an_hour(app),
                    "resume" => resume_now(app),
                    "mute" => toggle_mute(app),
                    "autostart" => toggle_autostart(app),
                    "report" => open_url(&format!("{REPO_URL}/issues/new")),
                    "quit" => app.exit(0),
                    _ => {}
                });

            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(StdDuration::from_millis(250));
                tick(&handle);
            });

            // THRESHOLD_DEMO_NOTIFY=1 posts one notification, to check the path
            // that covers a locked screen without having to lock one.
            if std::env::var("THRESHOLD_DEMO_NOTIFY").is_ok() {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(StdDuration::from_secs(3));
                    let cue = Cue::at(
                        "GO DRINK WATER",
                        chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
                    );
                    notify_behind_the_lock(&handle, &cue);
                });
            }

            // THRESHOLD_DEMO=N fires N cards, six seconds apart, four seconds
            // after boot — enough to see whether the second one is as clean as
            // the first without waiting for a cue or clicking a tray.
            if let Ok(count) = std::env::var("THRESHOLD_DEMO") {
                let count: usize = count.parse().unwrap_or(1);
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(StdDuration::from_secs(4));
                    let state = handle.state::<Mutex<Runtime>>();
                    let (font, floor) = {
                        let rt = match state.lock() {
                            Ok(rt) => rt,
                            Err(_) => return,
                        };
                        (rt.font.clone(), rt.store.config().floor_ms)
                    };
                    for n in 0..count {
                        trace(&format!("demo {}/{count}", n + 1));
                        let cue = Cue::at(
                            "GO DRINK WATER",
                            chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
                        );
                        fire_card(&handle, &font, &cue, floor);
                        std::thread::sleep(StdDuration::from_secs(6));
                    }
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // closing the settings window hides it; the app lives in the tray
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "settings" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            save_state,
            reload_config,
            fire_preview,
            card_floor,
            card_end,
            preview_cue,
            add_examples,
            open_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running Threshold");
}
