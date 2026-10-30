//! The shell: windows, tray, the tick, and the OS edges.
//!
//! Everything that decides *what* happens lives in `threshold-core`. This file
//! decides *where* it happens — which monitor, which window, whether input is
//! allowed through — and makes the noise.

pub mod audio;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration as StdDuration;

use chrono::{DateTime, Local};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, Wry,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as AutostartExt};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;
use threshold_core::{
    plan_tick, Anchors, Config, ConfigError, Cue, CueDraft, Font, Layout, OverlayMode, Schedule,
    Store, WrapSpec,
};

type RuntimeState = Mutex<Runtime>;

/// A poisoned lock must not silently disable the scheduler. A panic while the
/// state is open is already reported by the crash hook; the next tick should
/// still be able to run.
fn lock_rt(mutex: &RuntimeState) -> MutexGuard<'_, Runtime> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

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
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
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

/// The body of the "Report a problem" issue, with the last crash attached when
/// one exists. Nothing from the cue list is included.
fn problem_url(dir: &std::path::Path) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let mut body = format!(
        "Threshold {version} on {}\n\nWhat happened?\n\n",
        std::env::consts::OS
    );
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut crashes: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("crash-") && n.ends_with(".log"))
            })
            .collect();
        crashes.sort();
        if let Some(last) = crashes.last() {
            if let Ok(text) = std::fs::read_to_string(last) {
                let trimmed: String = text.chars().take(1_500).collect();
                body.push_str(&format!("Last crash log:\n\n```\n{trimmed}\n```\n"));
            }
        }
    }
    format!(
        "{REPO_URL}/issues/new?title={}&body={}",
        percent_encode("Issue"),
        percent_encode(&body)
    )
}

/// Alpha software should not die silently. A panic writes a timestamped report
/// next to the config. It does not open a browser by itself: a startup loop
/// would open one browser per panic. The tray's **Report a problem** is the
/// explicit path to a prefilled issue.
fn install_crash_reporter(dir: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let stamp = Local::now();
        let report = format!(
            "threshold {}\n{}\n{}\n\n{}",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            stamp.to_rfc3339(),
            info
        );
        let name = format!("crash-{}.log", stamp.format("%Y%m%d-%H%M%S"));
        let path = dir.join(name);
        let _ = std::fs::write(&path, &report);
        eprintln!("threshold: crashed, wrote {}", path.display());

        previous(info);
    }));
}
/// The way out when the UI is unusable: an OS-level chord that clears every
/// card and gives input back, whether or not the app wants to cooperate.
fn escape_shortcut() -> Shortcut {
    Shortcut::new(
        Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT),
        Code::KeyQ,
    )
}

fn escape_hatch(app: &AppHandle) {
    let state = app.state::<RuntimeState>();
    let (labels, permanent, focus_back) = {
        let mut rt = lock_rt(&state);
        let labels: Vec<String> = rt.overlays.iter().map(|o| o.label.clone()).collect();
        rt.active = None;
        rt.active_deadline = None;
        rt.pending.clear();
        rt.paused_until = Some(Local::now() + chrono::Duration::hours(1));
        (labels, rt.permanent, rt.focus_back.take())
    };

    for label in labels {
        let _ = app.emit_to(&label, "card_clear", ());
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.set_ignore_cursor_events(true);
            if !permanent {
                let _ = w.hide();
            }
        }
    }
    if let Some(handle) = focus_back {
        let _ = focus_window(handle);
    }
    {
        let rt = lock_rt(&state);
        refresh_tray(app, &rt);
    }
    trace("escape hatch: cards cleared, paused for an hour");
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
    /// Always-up layers, or windows shown only while a card is. Resolved from
    /// the config at boot and when the setting changes.
    permanent: bool,
    /// No cards at all until this instant. The impolite half.
    paused_until: Option<DateTime<Local>>,
    /// The layer currently showing a card, if any, and the window that had focus
    /// before it took over — so the keyboard can be handed back.
    active: Option<String>,
    /// When the active card must be over. Rust releases the layer if the page
    /// never calls `card_end`.
    active_deadline: Option<DateTime<Local>>,
    focus_back: Option<isize>,
    /// Cues that came due while another card was up: one card at a time.
    pending: VecDeque<Cue>,
    /// The next moment each cue is due. Rebuilt on reload, advanced on fire.
    next: HashMap<u64, Option<DateTime<Local>>>,
    /// Quarter-second ticks, used to refresh the tray tooltip occasionally.
    ticks: u64,
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
    overlay: String,
    cues: Vec<CueDraft>,
    /// When each cue is next due, RFC 3339, aligned with `cues`.
    next: Vec<Option<String>>,
    trouble: Option<String>,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CardPayload {
    /// The layer this card belongs to. An event reaches every webview that
    /// listens, so the page checks its own label rather than trusting that only
    /// the intended window was addressed.
    target: String,
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
    let state = app.try_state::<RuntimeState>().ok_or("starting")?;
    let rt = lock_rt(&state);
    let now = Local::now();
    let scheduled = rt.cues();
    Ok(StateDto {
        config_path: rt.store.path().display().to_string(),
        changed_on_disk: rt.store.changed_on_disk(),
        floor_ms: rt.store.config().floor_ms,
        volume: rt.store.config().volume,
        overlay: rt.store.config().overlay.as_str().to_string(),
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
    rt: State<'_, RuntimeState>,
    floor_ms: u32,
    volume: f32,
    overlay: String,
    cues: Vec<CueDraft>,
    force: Option<bool>,
) -> Result<(), String> {
    let mut rt = lock_rt(&rt);
    if rt.store.changed_on_disk() && !force.unwrap_or(false) {
        return Err(
            "cues.toml changed on disk since it was loaded; reload it, or overwrite it on purpose"
                .into(),
        );
    }
    let mut config = rt.store.config().clone();
    config.floor_ms = floor_ms;
    config.volume = volume.clamp(0.0, 1.0);
    config.overlay = OverlayMode::parse(&overlay)?;
    config.from_drafts(&cues).map_err(|e| e.to_string())?;

    // A cue that kept its identity keeps its anchor. A cue that was re-enabled
    // (or is new) starts again, which is how a spent one-shot is re-armed.
    let old: HashMap<String, bool> = rt
        .store
        .config()
        .cues
        .iter()
        .map(|c| (Anchors::key(c), c.enabled))
        .collect();
    for cue in &config.cues {
        match old.get(&Anchors::key(cue)) {
            Some(false) if cue.enabled => rt.anchors.forget(cue),
            Some(_) => {}
            None => rt.anchors.forget(cue),
        }
    }

    rt.store.save(config).map_err(|e| e.to_string())?;
    rt.trouble = None;
    rt.arm_missing();
    rebuild_next(&mut rt);

    // the mode can change while running: put the layers where they belong
    let want_permanent = rt.store.config().overlay.permanent_on(rt.overlays.len());
    if want_permanent != rt.permanent {
        rt.permanent = want_permanent;
        for o in &rt.overlays {
            if let Some(w) = app.get_webview_window(&o.label) {
                if want_permanent {
                    let _ = w.show();
                } else if rt.active.as_deref() != Some(o.label.as_str()) {
                    let _ = w.hide();
                }
            }
        }
    }

    refresh_tray(&app, &rt);
    Ok(())
}

#[tauri::command]
fn reload_config(app: AppHandle, rt: State<'_, RuntimeState>) -> Result<(), String> {
    let mut rt = lock_rt(&rt);
    rt.store.reload().map_err(|e| e.to_string())?;
    rt.trouble = None;
    rt.arm_missing();
    rebuild_next(&mut rt);
    refresh_tray(&app, &rt);
    Ok(())
}

#[tauri::command]
fn fire_preview(app: AppHandle, rt: State<'_, RuntimeState>) -> Result<(), String> {
    let (font, cue, floor) = {
        let rt = lock_rt(&rt);
        let cue = rt.store.config().cues.first().cloned().unwrap_or_else(|| {
            Cue::at(
                "THRESHOLD",
                chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
            )
        });
        (rt.font.clone(), cue, rt.store.config().floor_ms)
    };
    fire_card(&app, &font, &cue, floor);
    Ok(())
}

/// Fire one cue exactly as it is being edited, without saving it. The settings
/// list is the only place a card can be judged without waiting for its time.
#[tauri::command]
fn preview_cue(
    app: AppHandle,
    rt: State<'_, RuntimeState>,
    cue: CueDraft,
    floor_ms: Option<u32>,
) -> Result<(), String> {
    let (font, floor, cue) = {
        let rt = lock_rt(&rt);
        let cue = cue.to_cue(0)?;
        (
            rt.font.clone(),
            floor_ms.unwrap_or(rt.store.config().floor_ms),
            cue,
        )
    };
    fire_card(&app, &font, &cue, floor);
    Ok(())
}

/// The starter cues as drafts, so the settings window can append them to the
/// list being edited instead of saving over unsaved work.
#[tauri::command]
fn starter_cues() -> Vec<CueDraft> {
    Config::starter().drafts()
}

/// The Floor is over: input now lands on the card, and it takes the keyboard.
#[tauri::command]
fn card_floor(app: AppHandle, label: String) {
    let state = app.state::<RuntimeState>();
    let active = lock_rt(&state).active.clone();
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
/// pass-through, the keyboard goes back, and an on-demand layer leaves the screen.
#[tauri::command]
fn card_end(app: AppHandle, label: String) {
    end_card(&app, &label);
}

/// Ending is idempotent and never conditional on this layer still being the
/// active one: a second cue landing on another monitor must not leave the first
/// layer accepting clicks forever with nothing drawn on it.
fn end_card(app: &AppHandle, label: &str) {
    let state = app.state::<RuntimeState>();
    let (previous, permanent) = {
        let mut rt = lock_rt(&state);
        let was_active = rt.active.as_deref() == Some(label);
        if was_active {
            rt.active = None;
            rt.active_deadline = None;
        }
        (
            if was_active {
                rt.focus_back.take()
            } else {
                None
            },
            rt.permanent,
        )
    };

    if let Some(w) = app.get_webview_window(label) {
        let _ = w.set_ignore_cursor_events(true);
        if !permanent {
            let _ = w.hide();
        }
    }

    if let Some(handle) = previous {
        if !focus_window(handle) {
            // The keyboard must not be left on an invisible layer, and the
            // settings window must not be dragged in front of the user's work
            // just because focus could not be restored. If it is open, it is a
            // better owner than a click-through overlay.
            if let Some(settings) = app.get_webview_window("settings") {
                if settings.is_visible().unwrap_or(false) {
                    let _ = settings.set_focus();
                }
            }
        }
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
fn focus_window(handle: isize) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
    // Windows refuses this for a process that is not already foreground, so the
    // answer matters: a silent failure leaves the keyboard on an invisible layer.
    unsafe { SetForegroundWindow(HWND(handle as *mut _)).as_bool() }
}

#[cfg(not(windows))]
fn foreground_window() -> Option<isize> {
    None
}

#[cfg(not(windows))]
fn focus_window(_handle: isize) -> bool {
    false
}

fn fire_card(app: &AppHandle, font: &Font, cue: &Cue, floor_ms: u32) {
    // Which layer is under the pointer right now. Each one was born at its own
    // monitor's size, so nothing here resizes anything.
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());

    let state = app.state::<RuntimeState>();
    let (overlays, muted, volume, permanent, previous, focus_back) = {
        let mut rt = lock_rt(&state);
        // Taking focus is part of the interruption, but a second card landing
        // while one is up must not record the overlay itself as "where to go
        // back to".
        let capture = rt.active.is_none();
        if capture {
            rt.focus_back = foreground_window();
        }
        (
            rt.overlays.clone(),
            rt.muted,
            rt.store.config().volume,
            rt.permanent,
            rt.active.clone(),
            rt.focus_back,
        )
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
    let Some(window) = app.get_webview_window(&target.label) else {
        return;
    };
    // a stale interactive layer from an earlier card must not swallow this one
    let _ = window.set_ignore_cursor_events(true);

    let width_css = target.size.width as f32 / target.scale as f32;
    let Layout { lines, size_px } =
        threshold_core::layout::wrap(font, &cue.text.to_uppercase(), width_css, &WrapSpec::hd());

    let payload = CardPayload {
        target: target.label.clone(),
        lines,
        size_px,
        seconds: cue.seconds,
        floor_ms,
    };

    // A locked screen cannot show a window, so a card fired now would be spent
    // unseen. The sting still plays; the notification is what reaches them.
    let locked = screen_is_locked();
    trace(&format!("fire \"{}\" locked={locked}", cue.text));

    if !locked {
        // One card at a time: a layer that was showing the previous card is
        // cleared, and only the addressed layer draws this one.
        if let Some(prev) = previous.as_deref() {
            if !prev.is_empty() && prev != target.label {
                let _ = app.emit_to(prev, "card_clear", ());
            }
        }
        if !permanent {
            let _ = window.show();
        }
        let _ = app.emit_to(&target.label, "card", payload);

        let mut rt = lock_rt(&state);
        rt.focus_back = focus_back;
        rt.active = Some(target.label.clone());
        rt.active_deadline = Some(
            Local::now()
                + chrono::Duration::seconds(cue.seconds as i64)
                + chrono::Duration::milliseconds(floor_ms as i64)
                + chrono::Duration::seconds(2),
        );
    }

    if !muted {
        audio::fire(cue.sting, volume);
    }

    if locked {
        notify_behind_the_lock(app, cue);
    }

    // The page owns the transition: it calls `card_floor` at the Floor and
    // `card_end` when the Window is over. `active_deadline` is the watchdog for
    // the case where it never does.
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

    /// Give every cue that needs one an anchor: an interval starts its chain
    /// now, and a one-shot is pinned to the occurrence it is armed for. Without
    /// this a restart would re-arm a spent one-shot or re-phase an interval.
    fn arm_missing(&mut self) {
        let now = Local::now();
        let cues = self.store.config().cues.clone();
        for cue in &cues {
            let needs = match cue.schedule {
                Schedule::Every(_) => true,
                Schedule::At(_) => cue.is_one_shot(),
            };
            if needs && self.anchors.get(cue).is_none() {
                let at = match cue.schedule {
                    Schedule::Every(_) => Some(now),
                    _ => cue.next_fire(now),
                };
                if let Some(at) = at {
                    self.anchors.set(cue, at);
                }
            }
        }
        self.anchors.prune(&cues);
        if let Err(e) = self.anchors.save(&self.state_path) {
            eprintln!("threshold: could not persist the interval anchors: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// the tick
// ---------------------------------------------------------------------------

fn tick(app: &AppHandle) {
    let state = app.state::<RuntimeState>();
    let (stale, fire, font, floor_ms, changed, refresh_due) = {
        let mut rt = lock_rt(&state);
        let now = Local::now();
        rt.ticks = rt.ticks.wrapping_add(1);
        let refresh_due = rt.ticks % 120 == 1;

        // The page normally ends a card; this is what guarantees the layer gives
        // input back if it does not.
        let stale = rt
            .active
            .clone()
            .filter(|_| rt.active_deadline.is_some_and(|deadline| deadline <= now));

        let paused = rt.paused_until.is_some_and(|until| until > now);
        if rt.paused_until.is_some_and(|until| until <= now) {
            rt.paused_until = None;
        }

        let cues = rt.cues();
        let plan = plan_tick(&cues, &rt.next, now, paused);
        rt.next = plan.next;

        let mut changed = false;
        if paused {
            // A pause is an emergency stop, not a queue: cues due inside it are
            // advanced past rather than replayed when it ends.
            rt.pending.clear();
        } else {
            rt.pending.extend(plan.fire);
        }
        for (cue, due) in plan.anchors {
            rt.anchors.set(&cue, due);
            changed = true;
        }
        if changed {
            if let Err(e) = rt.anchors.save(&rt.state_path) {
                eprintln!("threshold: could not persist the interval anchors: {e}");
            }
        }

        // One card at a time, and never start one over a card that is still up
        // or one the watchdog is about to release.
        let fire = if stale.is_none() && rt.active.is_none() {
            rt.pending.pop_front()
        } else {
            None
        };
        if fire.is_some() {
            changed = true;
        }
        (
            stale,
            fire,
            rt.font.clone(),
            rt.store.config().floor_ms,
            changed,
            refresh_due,
        )
    };

    if let Some(ref label) = stale {
        trace(&format!("watchdog end {label}"));
        end_card(app, label);
    }
    if let Some(cue) = fire {
        fire_card(app, &font, &cue, floor_ms);
    }
    if changed || stale.is_some() || refresh_due {
        let rt = lock_rt(&state);
        refresh_tray(app, &rt);
    }
}

// ---------------------------------------------------------------------------
// tray
// ---------------------------------------------------------------------------

fn refresh_tray(app: &AppHandle, rt: &Runtime) {
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    let now = Local::now();
    let cues = rt.cues();
    let next = cues
        .iter()
        .filter(|c| c.enabled)
        .filter_map(|c| c.next_fire(now).map(|at| (at, &c.text)))
        .min_by_key(|(at, _)| *at);

    let tooltip = match next {
        Some((at, text)) => {
            let mins = (at - now).num_minutes().max(1);
            if mins < 60 {
                format!("Threshold · {text} in {mins}m")
            } else if mins < 60 * 24 {
                format!("Threshold · {text} in {}h{:02}m", mins / 60, mins % 60)
            } else {
                format!(
                    "Threshold · {text} in {}d {}h",
                    mins / (60 * 24),
                    (mins % (60 * 24)) / 60
                )
            }
        }
        None => "Threshold · nothing armed".to_string(),
    };
    let _ = tray.set_tooltip(Some(tooltip));
}

fn fire_first(app: &AppHandle) {
    let state = app.state::<RuntimeState>();
    let (font, cue, floor) = {
        let rt = lock_rt(&state);
        match rt.store.config().cues.first().cloned() {
            Some(cue) => (rt.font.clone(), cue, rt.store.config().floor_ms),
            None => return,
        }
    };
    fire_card(app, &font, &cue, floor);
}

fn pause_for_an_hour(app: &AppHandle) {
    let state = app.state::<RuntimeState>();
    let mut rt = lock_rt(&state);
    rt.paused_until = Some(Local::now() + chrono::Duration::hours(1));
    rt.pending.clear();
    refresh_tray(app, &rt);
}

fn resume_now(app: &AppHandle) {
    let state = app.state::<RuntimeState>();
    let mut rt = lock_rt(&state);
    rt.paused_until = None;
    rt.pending.clear();
    rebuild_next(&mut rt);
    refresh_tray(app, &rt);
}

fn toggle_mute(app: &AppHandle) {
    let state = app.state::<RuntimeState>();
    let muted = {
        let mut rt = lock_rt(&state);
        rt.muted = !rt.muted;
        rt.muted
    };
    if let Some(item) = app
        .state::<MuteItem>()
        .0
        .lock()
        .ok()
        .and_then(|g| g.clone())
    {
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
    let result = if want {
        manager.enable()
    } else {
        manager.disable()
    };

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
    let state = app.state::<RuntimeState>();
    let mut rt = lock_rt(&state);
    if let Err(e) = rt.store.reload() {
        eprintln!("threshold: config reload failed: {e}");
        return;
    }
    rt.trouble = None;
    rt.arm_missing();
    rebuild_next(&mut rt);
    refresh_tray(app, &rt);
}

// ---------------------------------------------------------------------------
// boot
// ---------------------------------------------------------------------------

fn config_path() -> PathBuf {
    if let Some(dirs) = directories::BaseDirs::new() {
        let preferred = dirs.config_dir().join("threshold").join("cues.toml");
        // One-time move from the pre-release `dev/threshold/threshold` location.
        if !preferred.exists() {
            if let Some(old) = directories::ProjectDirs::from("dev", "threshold", "threshold")
                .map(|d| d.config_dir().join("cues.toml"))
                .filter(|p| p.exists())
            {
                if let Some(parent) = preferred.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::copy(&old, &preferred);
                let state = old.with_file_name("state.toml");
                if state.exists() {
                    let _ = std::fs::copy(&state, preferred.with_file_name("state.toml"));
                }
            }
        }
        return preferred;
    }
    PathBuf::from("cues.toml")
}

/// A broken config must not stop the app from starting, and a transient read or
/// permission error must not move a perfectly good file aside.
fn open_store(path: &std::path::Path) -> (Store, Option<String>) {
    let seed = || Config::starter_at(Local::now());
    match Store::open_with_seed(path, seed()) {
        Ok(store) => (store, None),
        Err(e) => {
            let corrupt = matches!(
                &e,
                ConfigError::Toml { .. }
                    | ConfigError::Cue { .. }
                    | ConfigError::Version { .. }
                    | ConfigError::Invalid(_)
            );
            if !corrupt {
                return (
                    Store::from_config(path, seed()),
                    Some(format!(
                        "{e}\nThe app is running from an in-memory copy; fix the file and use Reload config."
                    )),
                );
            }

            let stamp = Local::now().format("%Y%m%d-%H%M%S");
            let mut backup = path.with_file_name(format!("cues.toml.broken-{stamp}"));
            let mut n = 1;
            while backup.exists() {
                backup = path.with_file_name(format!("cues.toml.broken-{stamp}-{n}"));
                n += 1;
            }
            let kept = std::fs::rename(path, &backup).is_ok();
            match Store::open_with_seed(path, seed()) {
                Ok(store) => (
                    store,
                    Some(if kept {
                        format!("{e}\nThe old file was kept as {}.", backup.display())
                    } else {
                        format!("{e}\nThe old file could not be moved aside.")
                    }),
                ),
                Err(write_error) => (
                    Store::from_config(path, seed()),
                    Some(format!(
                        "{e}\nA fresh config could not be written: {write_error}\nThe app is running from an in-memory copy."
                    )),
                ),
            }
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
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed && *shortcut == escape_shortcut() {
                        escape_hatch(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let path = config_path();
            install_crash_reporter(
                path.parent()
                    .unwrap_or(std::path::Path::new("."))
                    .to_path_buf(),
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
            //
            // The escape hatch is a safety feature, not a startup requirement: if
            // another app owns the chord, Threshold still runs and says so.
            let shortcut_trouble =
                app.global_shortcut()
                    .register(escape_shortcut())
                    .err()
                    .map(|e| {
                        format!(
                        "Ctrl+Alt+Shift+Q is already taken, so the escape hatch is unavailable: {e}"
                    )
                    });

            let mut runtime = Runtime {
                store,
                anchors,
                state_path,
                overlays: Vec::new(),
                muted: false,
                permanent: false,
                paused_until: None,
                active: None,
                active_deadline: None,
                focus_back: None,
                pending: VecDeque::new(),
                next: HashMap::new(),
                ticks: 0,
                font: Arc::new(font),
                trouble: trouble.or(shortcut_trouble),
            };
            runtime.arm_missing();
            rebuild_next(&mut runtime);
            app.manage(Mutex::new(runtime));

            // One transparent layer per monitor, born at that monitor's size.
            // Nothing is ever resized, so nothing can reflow or stretch.
            //
            // Whether they stay on screen depends on the mode: a permanent layer
            // is invisible when idle but always composed; on-demand keeps nothing
            // on screen between cards.
            let mut overlays = Vec::new();
            let monitors = app.available_monitors().unwrap_or_default();
            let permanent = {
                let state = app.state::<RuntimeState>();
                let mut rt = lock_rt(&state);
                rt.permanent = rt.store.config().overlay.permanent_on(monitors.len());
                rt.permanent
            };
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
                .visible(permanent)
                .focused(false)
                .position(
                    monitor.position().x as f64 / scale,
                    monitor.position().y as f64 / scale,
                )
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
                let state = app.state::<RuntimeState>();
                lock_rt(&state).overlays = overlays.clone();
            }

            // A permanent layer is always up; only an on-demand layer is shown
            // and hidden around a card. The hide/show path is opt-in because a
            // hidden webview can repaint blank on the frame it comes back.

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
            let mute =
                CheckMenuItem::with_id(app, "mute", "Mute the sting", true, false, None::<&str>)?;
            app.manage(MuteItem(Mutex::new(Some(mute.clone()))));
            let hint = MenuItem::with_id(
                app,
                "hint",
                "Escape hatch: Ctrl+Alt+Shift+Q",
                false,
                None::<&str>,
            )?;
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
                    &hint,
                    &quit,
                ],
            )?;

            let report_dir = path
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf();
            let mut tray = TrayIconBuilder::with_id("main")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "settings" => open_settings(app.clone()),
                    "fire" => fire_first(app),
                    "reload" => reload_runtime(app),
                    "pause" => pause_for_an_hour(app),
                    "resume" => resume_now(app),
                    "mute" => toggle_mute(app),
                    "autostart" => toggle_autostart(app),
                    "report" => {
                        open_url(&problem_url(&report_dir));
                    }
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
                // A panic in one tick must not silently kill the scheduler. The
                // crash hook has already written the report; keep the next tick.
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    tick(&handle);
                }));
                if result.is_err() {
                    eprintln!("threshold: a tick panicked; the scheduler is still running");
                }
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
                    let state = handle.state::<RuntimeState>();
                    let (font, floor) = {
                        let rt = lock_rt(&state);
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

            // The settings window is created hidden so an autostart launch goes
            // straight to the tray. A normal launch shows it once.
            let at_login = std::env::args().any(|a| a == "--autostart");
            if !at_login {
                if let Some(window) = app.get_webview_window("settings") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // closing the settings window hides it; the app lives in the tray
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "settings" {
                    api.prevent_close();
                    let _ = window.hide();
                } else if window.label().starts_with("overlay-") {
                    // an overlay has no decorations, but Alt+F4 still arrives
                    api.prevent_close();
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
            starter_cues,
            open_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running Threshold");
}

#[cfg(test)]
mod dto_tests {
    use super::*;

    #[test]
    fn the_state_dto_has_every_field_the_settings_window_reads() {
        let dto = StateDto {
            config_path: "cues.toml".into(),
            changed_on_disk: false,
            floor_ms: 600,
            volume: 0.8,
            overlay: "permanent".into(),
            cues: vec![],
            next: vec![],
            trouble: None,
        };
        let json = serde_json::to_value(dto).unwrap();
        for key in [
            "configPath",
            "changedOnDisk",
            "floorMs",
            "volume",
            "overlay",
            "cues",
            "next",
            "trouble",
        ] {
            assert!(json.get(key).is_some(), "StateDto is missing {key}: {json}");
        }
    }

    #[test]
    fn the_card_payload_matches_the_overlay_component() {
        let payload = CardPayload {
            target: "overlay-0".into(),
            lines: vec!["GO DRINK".into(), "WATER".into()],
            size_px: 180.0,
            seconds: 4,
            floor_ms: 600,
        };
        let json = serde_json::to_value(payload).unwrap();
        for key in ["target", "lines", "sizePx", "seconds", "floorMs"] {
            assert!(
                json.get(key).is_some(),
                "CardPayload is missing {key}: {json}"
            );
        }
    }
}
