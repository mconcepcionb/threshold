//! The Config file: the Cue list as TOML.
//!
//! Two writers exist on purpose — the settings window and your text editor — so
//! this module owns the least-bad answer to that: the app always writes whole
//! files atomically, the file is fingerprinted so an external edit can be
//! detected before anything is overwritten, and a parse error never discards a
//! config that was loading fine a second ago.
//!
//! The on-disk form is deliberately forgiving: two required fields per Cue, six
//! optional ones (`repeat`, `late`, `seconds`, `sting`, `enabled`, and the
//! internal `anchor`), with defaults that mean a hand-written cue is two lines.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Duration, Local, NaiveTime};
use serde::{Deserialize, Serialize};

use crate::cue::{Cue, LatePolicy, Repeat, Schedule};

pub const FORMAT_VERSION: u32 = 1;

/// How the card reaches the screen. A permanent layer is invisible when idle and
/// costs one composition per monitor; showing a window on demand keeps nothing on
/// screen between cards, at the risk of a first-frame flash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverlayMode {
    /// Permanent layer on multi-monitor machines, on demand on a single screen.
    #[default]
    Auto,
    Permanent,
    OnDemand,
}

impl OverlayMode {
    pub fn parse(s: &str) -> Result<OverlayMode, String> {
        match s.trim() {
            "auto" | "" => Ok(OverlayMode::Auto),
            "permanent" => Ok(OverlayMode::Permanent),
            "on_demand" | "on-demand" => Ok(OverlayMode::OnDemand),
            other => Err(format!(
                "overlay `{other}` is not auto, permanent or on_demand"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            OverlayMode::Auto => "auto",
            OverlayMode::Permanent => "permanent",
            OverlayMode::OnDemand => "on_demand",
        }
    }

    /// What the mode means on a machine with `monitors` screens.
    pub fn permanent_on(self, monitors: usize) -> bool {
        match self {
            OverlayMode::Permanent => true,
            OverlayMode::OnDemand => false,
            OverlayMode::Auto => monitors > 1,
        }
    }
}
pub const DEFAULT_FLOOR_MS: u32 = 600;
pub const MAX_TEXT_CHARS: usize = 40;
/// The UI and the starter file agree on this range; the model enforces it.
pub const MAX_SEMITONES: f32 = 6.0;
const MAX_INTERVAL_SECS: i64 = 30 * 24 * 60 * 60;
const MAX_GRACE_SECS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub floor_ms: u32,
    pub overlay: OverlayMode,
    /// Sting level, 0.0 to 1.0. One knob, because the sound is one sound.
    pub volume: f32,
    pub cues: Vec<Cue>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            floor_ms: DEFAULT_FLOOR_MS,
            overlay: OverlayMode::Auto,
            volume: 1.0,
            cues: Vec::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} is not valid TOML: {message}")]
    Toml { path: PathBuf, message: String },
    #[error("cue {index}: {message}")]
    Cue { index: usize, message: String },
    #[error("config version {found} is newer than this build understands ({ours})")]
    Version { found: u32, ours: u32 },
    #[error("{0}")]
    Invalid(String),
}

// --- on-disk shape ---------------------------------------------------------
// Strings, not enums: `at = "18:00"` and `late = "catch_up(10m)"` are easier to
// type by hand than tables, and the error messages can name the field.

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default = "default_floor")]
    floor_ms: u32,
    #[serde(default = "default_volume")]
    volume: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    overlay: Option<String>,
    #[serde(default, rename = "cue", skip_serializing_if = "Vec::is_empty")]
    cues: Vec<CueFile>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CueFile {
    text: String,
    at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repeat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    late: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seconds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sting: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    anchor: Option<String>,
}

fn default_version() -> u32 {
    FORMAT_VERSION
}
fn default_floor() -> u32 {
    DEFAULT_FLOOR_MS
}
fn default_volume() -> f32 {
    1.0
}

impl Config {
    /// The twelve cues shipped in `examples/cues.toml`, parsed at build time.
    /// A fresh install has something to fire the moment it starts.
    pub fn starter() -> Config {
        const STARTER: &str = include_str!("../../examples/cues.toml");
        Config::parse(Path::new("examples/cues.toml"), STARTER).unwrap_or_default()
    }

    /// The starter cues, plus one cue due shortly after `now`.
    ///
    /// A fresh install should prove itself while the person is still looking at
    /// it. The twelve examples teach the format but they are wall-clock times,
    /// so the first thing you would see could be hours away — this one arrives in
    /// ninety seconds, and a one-shot disarms itself once it has fired.
    pub fn starter_at(now: DateTime<Local>) -> Config {
        let mut config = Config::starter();

        // Ninety seconds out. A one-shot is a time today, so if that would land
        // tomorrow (a fresh install at 23:59) there is no arrival cue at all —
        // the twelve examples still fire tomorrow, and a card that cannot be
        // scheduled is better than one scheduled for the wrong day.
        let due = now + Duration::seconds(90);
        if due.date_naive() != now.date_naive() {
            return config;
        }
        let at = due.time();

        config.cues.push(Cue {
            id: config.cues.len() as u64 + 1,
            text: "YOU ARE HERE".to_string(),
            schedule: Schedule::At(at),
            repeat: Repeat::Once,
            late: LatePolicy::Exact,
            seconds: 4,
            sting: 0.0,
            enabled: true,
            anchor: None,
        });
        config
    }

    /// Append the starter cues that are not already there, matched by text and
    /// schedule. Returns how many were added. Safe to run twice.
    pub fn merge_starter(&mut self) -> usize {
        let starter = Config::starter();
        let mut added = 0;
        for cue in starter.cues {
            let draft = CueDraft::from_cue(&cue);
            let present = self.cues.iter().any(|c| {
                let existing = CueDraft::from_cue(c);
                existing.text == draft.text && existing.at == draft.at
            });
            if !present {
                self.cues.push(cue);
                added += 1;
            }
        }
        // ids are positional; inserting must not leave two cues sharing one
        for (i, cue) in self.cues.iter_mut().enumerate() {
            cue.id = i as u64 + 1;
        }
        added
    }

    /// Parse a whole file. `index` in an error is the cue's position, 1-based,
    /// so it points at the `[[cue]]` block a human can find.
    pub fn parse(path: &Path, text: &str) -> Result<Config, ConfigError> {
        // Read the version before the strict struct: a newer file that added a
        // field should report "this build is too old", not "unknown field".
        let raw: toml::Value = toml::from_str(text).map_err(|e| ConfigError::Toml {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        if let Some(found) = raw.get("version").and_then(|v| v.as_integer()) {
            if found > FORMAT_VERSION as i64 {
                return Err(ConfigError::Version {
                    found: found as u32,
                    ours: FORMAT_VERSION,
                });
            }
        }

        let file: ConfigFile = toml::from_str(text).map_err(|e| ConfigError::Toml {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

        let mut cues = Vec::with_capacity(file.cues.len());
        for (i, raw) in file.cues.into_iter().enumerate() {
            let index = i + 1;
            cues.push(
                raw.into_cue(index)
                    .map_err(|message| ConfigError::Cue { index, message })?,
            );
        }

        let overlay = match file.overlay.as_deref() {
            None => OverlayMode::Auto,
            Some(s) => OverlayMode::parse(s).map_err(ConfigError::Invalid)?,
        };
        let config = Config {
            floor_ms: file.floor_ms,
            overlay,
            volume: file.volume,
            cues,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn to_toml(&self) -> Result<String, ConfigError> {
        self.validate()?;
        let file = ConfigFile {
            version: FORMAT_VERSION,
            floor_ms: self.floor_ms,
            overlay: (self.overlay != OverlayMode::Auto).then(|| self.overlay.as_str().to_string()),
            volume: self.volume,
            cues: self.cues.iter().map(CueFile::from_cue).collect(),
        };
        let header = "# Threshold cues. Edit here or in the app; both write this file.\n\
                      # text is required, at is required, everything else has a default.\n\
                      # at: \"18:00\" | \"every 25m\" | \"in 25m\"\n\
                      # late: \"exact\" | \"catch_up(10m)\" | \"next_opportunity\"\n\n";
        let body = toml::to_string_pretty(&file)
            .map_err(|e| ConfigError::Invalid(format!("could not serialise config: {e}")))?;
        Ok(format!("{header}{body}"))
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.floor_ms < 100 || self.floor_ms > 10_000 {
            return Err(ConfigError::Invalid(format!(
                "floor_ms must be between 100 and 10000, got {}",
                self.floor_ms
            )));
        }
        if !(0.0..=1.0).contains(&self.volume) {
            return Err(ConfigError::Invalid(format!(
                "volume must be between 0 and 1, got {}",
                self.volume
            )));
        }
        let mut seen = std::collections::HashSet::new();
        for (i, cue) in self.cues.iter().enumerate() {
            let n = i + 1;
            let chars = cue.text.chars().count();
            if cue.text.trim().is_empty() {
                return Err(ConfigError::Cue {
                    index: n,
                    message: "text is empty".into(),
                });
            }
            if chars > MAX_TEXT_CHARS {
                return Err(ConfigError::Cue {
                    index: n,
                    message: format!("text is {chars} characters; the limit is {MAX_TEXT_CHARS}"),
                });
            }
            if cue.seconds == 0 || cue.seconds > 120 {
                return Err(ConfigError::Cue {
                    index: n,
                    message: format!("seconds must be 1..=120, got {}", cue.seconds),
                });
            }
            if !cue.sting.is_finite() || cue.sting.abs() > MAX_SEMITONES {
                return Err(ConfigError::Cue {
                    index: n,
                    message: format!(
                        "sting must be a finite pitch offset in -{MAX_SEMITONES}..={MAX_SEMITONES}, got {}",
                        cue.sting
                    ),
                });
            }
            validate_schedule(cue, n)?;
            if !seen.insert(crate::state::Anchors::key(cue)) {
                return Err(ConfigError::Cue {
                    index: n,
                    message: "another cue has the same text and schedule; give one of them a different time or text"
                        .into(),
                });
            }
        }
        Ok(())
    }
}

fn validate_schedule(cue: &Cue, index: usize) -> Result<(), ConfigError> {
    let fail = |message: String| ConfigError::Cue { index, message };
    if let Schedule::Every(period) = cue.schedule {
        if period <= Duration::zero() || period.num_seconds() > MAX_INTERVAL_SECS {
            return Err(fail(format!(
                "interval must be between 1 second and {} days, got {}",
                MAX_INTERVAL_SECS / 86_400,
                format_duration(period)
            )));
        }
        if matches!(cue.repeat, Repeat::Weekly(ref days) if days.is_empty()) {
            return Err(fail("weekly() needs at least one day".into()));
        }
    }
    if let LatePolicy::CatchUp(grace) = cue.late {
        if grace <= Duration::zero() || grace.num_seconds() > MAX_GRACE_SECS {
            return Err(fail(format!(
                "catch_up grace must be between 1 second and {} days, got {}",
                MAX_GRACE_SECS / 86_400,
                format_duration(grace)
            )));
        }
    }
    Ok(())
}

impl CueFile {
    fn into_cue(self, index: usize) -> Result<Cue, String> {
        let schedule = parse_schedule(&self.at)?;
        let repeat = match self.repeat.as_deref() {
            None => Repeat::Once,
            Some(s) => parse_repeat(s)?,
        };
        let late = match self.late.as_deref() {
            None => LatePolicy::Exact,
            Some(s) => parse_late(s)?,
        };
        let anchor = match self.anchor.as_deref() {
            None => None,
            Some(s) => Some(
                DateTime::parse_from_rfc3339(s)
                    .map_err(|_| format!("anchor `{s}` is not an RFC 3339 timestamp"))?
                    .with_timezone(&Local),
            ),
        };

        Ok(Cue {
            id: index as u64,
            text: self.text.trim().to_string(),
            schedule,
            repeat,
            late,
            seconds: self.seconds.unwrap_or(4),
            sting: self.sting.unwrap_or(0.0),
            enabled: self.enabled.unwrap_or(true),
            anchor,
        })
    }

    fn from_cue(cue: &Cue) -> CueFile {
        CueFile {
            text: cue.text.clone(),
            at: format_schedule(&cue.schedule),
            repeat: match cue.repeat {
                Repeat::Once => None,
                _ => Some(format_repeat(&cue.repeat)),
            },
            late: match cue.late {
                LatePolicy::Exact => None,
                _ => Some(format_late(&cue.late)),
            },
            seconds: (cue.seconds != 4).then_some(cue.seconds),
            sting: (cue.sting != 0.0).then_some(cue.sting),
            enabled: (!cue.enabled).then_some(false),
            anchor: cue.anchor.map(|a| a.to_rfc3339()),
        }
    }
}

// --- field parsing ---------------------------------------------------------

fn parse_schedule(s: &str) -> Result<Schedule, String> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("every ") {
        return Ok(Schedule::Every(parse_duration(rest)?));
    }
    if let Some(rest) = s.strip_prefix("in ") {
        return Ok(Schedule::Every(parse_duration(rest)?));
    }
    NaiveTime::parse_from_str(s, "%H:%M")
        .map(Schedule::At)
        .map_err(|_| format!("at `{s}` is not HH:MM, \"every 25m\" or \"in 25m\""))
}

fn format_schedule(schedule: &Schedule) -> String {
    match schedule {
        Schedule::At(t) => t.format("%H:%M").to_string(),
        Schedule::Every(d) => format!("every {}", format_duration(*d)),
    }
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    let (num, unit) = s
        .char_indices()
        .next_back()
        .ok_or_else(|| "duration is empty".to_string())?;
    let num = &s[..num];
    let n: i64 = num
        .parse()
        .map_err(|_| format!("duration `{s}` must look like 30s, 25m or 2h"))?;
    if n <= 0 {
        return Err(format!("duration `{s}` must be positive"));
    }
    let out_of_range = || format!("duration `{s}` is too large");
    match unit {
        's' => Duration::try_seconds(n).ok_or_else(out_of_range),
        'm' => Duration::try_minutes(n).ok_or_else(out_of_range),
        'h' => Duration::try_hours(n).ok_or_else(out_of_range),
        _ => Err(format!("duration `{s}` must end in s, m or h")),
    }
}

fn format_duration(d: Duration) -> String {
    let ms = d.num_milliseconds();
    if ms % 3_600_000 == 0 {
        format!("{}h", ms / 3_600_000)
    } else if ms % 60_000 == 0 {
        format!("{}m", ms / 60_000)
    } else {
        format!("{}s", ms / 1000)
    }
}

fn parse_repeat(s: &str) -> Result<Repeat, String> {
    match s.trim() {
        "once" => Ok(Repeat::Once),
        "daily" => Ok(Repeat::Daily),
        "weekdays" => Ok(Repeat::Weekdays),
        other => {
            let inner = other
                .strip_prefix("weekly(")
                .and_then(|r| r.strip_suffix(')'))
                .ok_or_else(|| {
                    format!("repeat `{other}` is not once, daily, weekdays or weekly(mon,wed)")
                })?;
            let mut days = Vec::new();
            for part in inner.split(',') {
                days.push(parse_weekday(part.trim())?);
            }
            if days.is_empty() {
                return Err("weekly() needs at least one day".into());
            }
            Ok(Repeat::Weekly(days))
        }
    }
}

fn format_repeat(repeat: &Repeat) -> String {
    match repeat {
        Repeat::Once => "once".into(),
        Repeat::Daily => "daily".into(),
        Repeat::Weekdays => "weekdays".into(),
        Repeat::Weekly(days) => {
            let names: Vec<&str> = days.iter().map(|d| weekday_name(*d)).collect();
            format!("weekly({})", names.join(","))
        }
    }
}

fn parse_weekday(s: &str) -> Result<chrono::Weekday, String> {
    use chrono::Weekday::*;
    match s.to_ascii_lowercase().as_str() {
        "mon" => Ok(Mon),
        "tue" => Ok(Tue),
        "wed" => Ok(Wed),
        "thu" => Ok(Thu),
        "fri" => Ok(Fri),
        "sat" => Ok(Sat),
        "sun" => Ok(Sun),
        other => Err(format!("`{other}` is not a weekday abbreviation")),
    }
}

fn weekday_name(w: chrono::Weekday) -> &'static str {
    use chrono::Weekday::*;
    match w {
        Mon => "mon",
        Tue => "tue",
        Wed => "wed",
        Thu => "thu",
        Fri => "fri",
        Sat => "sat",
        Sun => "sun",
    }
}

fn parse_late(s: &str) -> Result<LatePolicy, String> {
    let s = s.trim();
    match s {
        "exact" => Ok(LatePolicy::Exact),
        "next_opportunity" => Ok(LatePolicy::NextOpportunity),
        other => {
            let inner = other
                .strip_prefix("catch_up(")
                .and_then(|r| r.strip_suffix(')'))
                .ok_or_else(|| {
                    format!("late `{other}` is not exact, catch_up(10m) or next_opportunity")
                })?;
            Ok(LatePolicy::CatchUp(parse_duration(inner)?))
        }
    }
}

fn format_late(late: &LatePolicy) -> String {
    match late {
        LatePolicy::Exact => "exact".into(),
        LatePolicy::NextOpportunity => "next_opportunity".into(),
        LatePolicy::CatchUp(d) => format!("catch_up({})", format_duration(*d)),
    }
}

// --- the file itself -------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

impl Stamp {
    fn of(path: &Path) -> Option<Stamp> {
        let meta = std::fs::metadata(path).ok()?;
        Some(Stamp {
            modified: meta.modified().ok(),
            len: meta.len(),
        })
    }
}

/// A Config that lives in a file, plus what is needed to know whether someone
/// else has written to it since we last looked.
#[derive(Debug)]
pub struct Store {
    path: PathBuf,
    config: Config,
    stamp: Option<Stamp>,
}

impl Store {
    /// Open, creating a default config if the file is missing.
    pub fn open(path: impl Into<PathBuf>) -> Result<Store, ConfigError> {
        Store::open_with_seed(path, Config::starter())
    }

    /// Open, writing `seed` if the file does not exist yet.
    pub fn open_with_seed(path: impl Into<PathBuf>, seed: Config) -> Result<Store, ConfigError> {
        let path = path.into();
        if !path.exists() {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|source| ConfigError::Io {
                    path: dir.to_path_buf(),
                    source,
                })?;
            }
            let text = seed.to_toml()?;
            let config = seed;
            write_atomic(&path, &text)?;
            let stamp = Stamp::of(&path);
            return Ok(Store {
                path,
                config,
                stamp,
            });
        }

        let text = std::fs::read_to_string(&path).map_err(|source| ConfigError::Io {
            path: path.clone(),
            source,
        })?;
        let config = Config::parse(&path, &text)?;
        let stamp = Stamp::of(&path);
        Ok(Store {
            path,
            config,
            stamp,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A store that holds `config` in memory without touching the disk. Used
    /// when the file cannot be read or written: the app still has a scheduler,
    /// and the settings window can explain what happened.
    pub fn from_config(path: impl Into<PathBuf>, config: Config) -> Store {
        let path = path.into();
        let stamp = Stamp::of(&path);
        Store {
            path,
            config,
            stamp,
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// True when the file on disk is no longer the one we loaded — someone
    /// edited it by hand. The settings window asks before saving over it.
    pub fn changed_on_disk(&self) -> bool {
        Stamp::of(&self.path) != self.stamp
    }

    pub fn reload(&mut self) -> Result<(), ConfigError> {
        let text = std::fs::read_to_string(&self.path).map_err(|source| ConfigError::Io {
            path: self.path.clone(),
            source,
        })?;
        self.config = Config::parse(&self.path, &text)?;
        self.stamp = Stamp::of(&self.path);
        Ok(())
    }

    /// Write the whole config, atomically: a half-written file is never visible.
    pub fn save(&mut self, config: Config) -> Result<(), ConfigError> {
        let text = config.to_toml()?;
        write_atomic(&self.path, &text)?;
        self.config = config;
        self.stamp = Stamp::of(&self.path);
        Ok(())
    }
}

fn write_atomic(path: &Path, text: &str) -> Result<(), ConfigError> {
    crate::fsutil::write_atomic(path, text).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })
}

// --- the shape a UI edits ------------------------------------------------
// The settings window works in the same strings the file uses, so a cue looks
// identical on both surfaces and neither can express something the other cannot.

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CueDraft {
    pub text: String,
    pub at: String,
    #[serde(default)]
    pub repeat: String,
    #[serde(default)]
    pub late: String,
    pub seconds: u32,
    pub sting: f32,
    pub enabled: bool,
}

impl CueDraft {
    pub fn from_cue(cue: &Cue) -> Self {
        CueDraft {
            text: cue.text.clone(),
            at: format_schedule(&cue.schedule),
            repeat: match cue.repeat {
                Repeat::Once => String::new(),
                _ => format_repeat(&cue.repeat),
            },
            late: match cue.late {
                LatePolicy::Exact => String::new(),
                _ => format_late(&cue.late),
            },
            seconds: cue.seconds,
            sting: cue.sting,
            enabled: cue.enabled,
        }
    }

    pub fn to_cue(&self, id: u64) -> Result<Cue, String> {
        let text = self.text.trim().to_string();
        if text.is_empty() {
            return Err("text is empty".into());
        }
        let chars = text.chars().count();
        if chars > MAX_TEXT_CHARS {
            return Err(format!(
                "text is {chars} characters; the limit is {MAX_TEXT_CHARS}"
            ));
        }
        if self.seconds == 0 || self.seconds > 120 {
            return Err(format!("seconds must be 1..=120, got {}", self.seconds));
        }
        if !self.sting.is_finite() || self.sting.abs() > MAX_SEMITONES {
            return Err(format!(
                "sting must be a finite pitch offset in -{MAX_SEMITONES}..={MAX_SEMITONES}, got {}",
                self.sting
            ));
        }
        Ok(Cue {
            id,
            text,
            schedule: parse_schedule(&self.at)?,
            repeat: if self.repeat.is_empty() {
                Repeat::Once
            } else {
                parse_repeat(&self.repeat)?
            },
            late: if self.late.is_empty() {
                LatePolicy::Exact
            } else {
                parse_late(&self.late)?
            },
            seconds: self.seconds,
            sting: self.sting,
            enabled: self.enabled,
            anchor: None,
        })
    }
}

impl Config {
    pub fn drafts(&self) -> Vec<CueDraft> {
        self.cues.iter().map(CueDraft::from_cue).collect()
    }

    /// Replace the cue list from drafts, keeping the existing anchors where the
    /// cue is unchanged so an interval chain is not re-phased by an edit.
    /// Re-enabling a cue that was disarmed clears its anchor, which re-arms a
    /// one-shot and starts an interval chain from now.
    pub fn from_drafts(&mut self, drafts: &[CueDraft]) -> Result<(), ConfigError> {
        let mut cues = Vec::with_capacity(drafts.len());
        for (i, draft) in drafts.iter().enumerate() {
            let index = i + 1;
            let mut cue = draft
                .to_cue(index as u64)
                .map_err(|message| ConfigError::Cue { index, message })?;
            let old = self.cues.get(i);
            let same = old
                .is_some_and(|o| crate::state::Anchors::key(o) == crate::state::Anchors::key(&cue));
            let rearmed = old.is_some_and(|o| !o.enabled && cue.enabled);
            cue.anchor = if same && !rearmed {
                old.and_then(|o| o.anchor)
            } else {
                None
            };
            cues.push(cue);
        }
        self.cues = cues;
        self.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Weekday;

    fn hm(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn temp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("threshold-test-{name}-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn a_minimal_cue_is_two_lines() {
        let config = Config::parse(
            Path::new("cues.toml"),
            "[[cue]]\ntext = \"GO DRINK WATER\"\nat = \"18:00\"\n",
        )
        .unwrap();

        assert_eq!(config.floor_ms, 600);
        let cue = &config.cues[0];
        assert_eq!(cue.text, "GO DRINK WATER");
        assert_eq!(cue.schedule, Schedule::At(hm(18, 0)));
        assert_eq!(cue.repeat, Repeat::Once);
        assert_eq!(cue.late, LatePolicy::Exact);
        assert_eq!(cue.seconds, 4);
        assert!(cue.enabled);
    }

    #[test]
    fn every_field_round_trips() {
        let text = r#"
floor_ms = 800

[[cue]]
text = "FOCUS"
at = "every 25m"
late = "catch_up(10m)"
repeat = "weekdays"
seconds = 6
sting = -3.0
enabled = false
"#;
        let config = Config::parse(Path::new("cues.toml"), text).unwrap();
        assert_eq!(config.floor_ms, 800);
        assert_eq!(
            config.cues[0].schedule,
            Schedule::Every(Duration::minutes(25))
        );
        assert_eq!(
            config.cues[0].late,
            LatePolicy::CatchUp(Duration::minutes(10))
        );
        assert_eq!(config.cues[0].repeat, Repeat::Weekdays);
        assert_eq!(config.cues[0].seconds, 6);
        assert_eq!(config.cues[0].sting, -3.0);
        assert!(!config.cues[0].enabled);

        let again = Config::parse(Path::new("cues.toml"), &config.to_toml().unwrap()).unwrap();
        assert_eq!(again, config);
    }

    #[test]
    fn weekly_days_round_trip() {
        let text = "[[cue]]\ntext = \"STANDUP\"\nat = \"09:30\"\nrepeat = \"weekly(tue,thu)\"\n";
        let config = Config::parse(Path::new("cues.toml"), text).unwrap();
        assert_eq!(
            config.cues[0].repeat,
            Repeat::Weekly(vec![Weekday::Tue, Weekday::Thu])
        );
        let again = Config::parse(Path::new("cues.toml"), &config.to_toml().unwrap()).unwrap();
        assert_eq!(again.cues[0].repeat, config.cues[0].repeat);
    }

    #[test]
    fn defaults_are_omitted_when_writing() {
        let config = Config::parse(
            Path::new("cues.toml"),
            "[[cue]]\ntext = \"REVIEW MAIL\"\nat = \"18:00\"\n",
        )
        .unwrap();
        let out = config.to_toml().unwrap();
        for field in ["repeat", "late", "seconds", "sting", "enabled", "anchor"] {
            assert!(
                !out.contains(&format!("\n{field} =")),
                "{field} should be omitted:\n{out}"
            );
        }
    }

    #[test]
    fn a_bad_time_names_the_cue_and_the_field() {
        let err = Config::parse(
            Path::new("cues.toml"),
            "[[cue]]\ntext = \"X\"\nat = \"6pm\"\n",
        )
        .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("cue 1"), "{message}");
        assert!(message.contains("at `6pm`"), "{message}");
    }

    #[test]
    fn a_typo_is_an_error_not_a_silent_drop() {
        let err = Config::parse(
            Path::new("cues.toml"),
            "[[cue]]\ntext = \"X\"\nat = \"18:00\"\nsecond = 6\n",
        )
        .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("unknown field"),
            "{err}"
        );
    }

    #[test]
    fn text_length_is_enforced_on_the_model() {
        let long = "A".repeat(MAX_TEXT_CHARS + 1);
        let err = Config::parse(
            Path::new("cues.toml"),
            &format!("[[cue]]\ntext = \"{long}\"\nat = \"18:00\"\n"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("limit"), "{err}");
    }

    #[test]
    fn store_creates_reads_and_saves() {
        let path = temp_path("store");
        let mut store = Store::open(&path).unwrap();
        assert_eq!(
            store.config().cues.len(),
            12,
            "a fresh config starts from the starter file"
        );

        let mut config = store.config().clone();
        config.cues.push(Cue::at("TEST CUE", hm(18, 0)));
        store.save(config).unwrap();
        assert!(!store.changed_on_disk());

        let reopened = Store::open(&path).unwrap();
        assert_eq!(reopened.config().cues.len(), 13);
        assert_eq!(reopened.config().cues.last().unwrap().text, "TEST CUE");

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn an_external_edit_is_noticed_but_a_failed_reload_keeps_the_old_config() {
        let path = temp_path("drift");
        let mut store = Store::open(&path).unwrap();
        let mut config = store.config().clone();
        config.cues.push(Cue::at("TEST CUE", hm(18, 0)));
        store.save(config).unwrap();

        // someone else writes the file
        std::fs::write(&path, "[[cue]]\ntext = \"FROM EDITOR\"\nat = \"19:00\"\n").unwrap();
        assert!(store.changed_on_disk());

        // and then breaks it: the in-memory config must survive
        std::fs::write(&path, "this is not toml").unwrap();
        assert!(store.reload().is_err());
        assert_eq!(store.config().cues.last().unwrap().text, "TEST CUE");

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_multibyte_duration_is_an_error_not_a_panic() {
        for at in ["every 5é", "in 5µ", "every é"] {
            let err = Config::parse(
                Path::new("cues.toml"),
                &format!("[[cue]]\ntext = \"X\"\nat = \"{at}\"\n"),
            )
            .unwrap_err();
            assert!(err.to_string().contains("cue 1"), "{err}");
        }
    }

    #[test]
    fn an_out_of_range_duration_is_an_error_not_a_panic() {
        let err = Config::parse(
            Path::new("cues.toml"),
            "[[cue]]\ntext = \"X\"\nat = \"every 999999999999999999m\"\n",
        )
        .unwrap_err();
        assert!(err.to_string().contains("duration"), "{err}");
    }

    #[test]
    fn a_newer_format_is_reported_before_unknown_fields() {
        let err = Config::parse(
            Path::new("cues.toml"),
            "version = 2\nfuture_field = true\n[[cue]]\ntext = \"X\"\nat = \"18:00\"\n",
        )
        .unwrap_err();
        assert!(
            matches!(err, ConfigError::Version { found: 2, ours: 1 }),
            "expected a version error, got {err}"
        );
    }

    #[test]
    fn a_non_finite_pitch_is_rejected() {
        let err = Config::parse(
            Path::new("cues.toml"),
            "[[cue]]\ntext = \"X\"\nat = \"18:00\"\nsting = nan\n",
        )
        .unwrap_err();
        assert!(err.to_string().contains("sting"), "{err}");
    }

    #[test]
    fn two_identical_cues_are_rejected() {
        let err = Config::parse(
            Path::new("cues.toml"),
            "[[cue]]\ntext = \"FOCUS\"\nat = \"every 25m\"\n\n[[cue]]\ntext = \"FOCUS\"\nat = \"every 25m\"\n",
        )
        .unwrap_err();
        assert!(err.to_string().contains("same text and schedule"), "{err}");
    }

    #[test]
    fn re_enabling_a_spent_one_shot_clears_its_anchor() {
        let mut config = Config::parse(
            Path::new("cues.toml"),
            "[[cue]]\ntext = \"ONCE\"\nat = \"09:00\"\n",
        )
        .unwrap();
        config.cues[0].anchor = Some(Local::now());
        config.cues[0].enabled = false;

        let mut draft = CueDraft::from_cue(&config.cues[0]);
        draft.enabled = true;
        config.from_drafts(&[draft]).unwrap();
        assert_eq!(
            config.cues[0].anchor, None,
            "re-arming starts a new occurrence"
        );
    }
}

#[cfg(test)]
mod starter_tests {
    use super::*;

    #[test]
    fn merging_examples_adds_only_what_is_missing() {
        let mut config = Config::default();
        assert_eq!(config.merge_starter(), 12);
        assert_eq!(config.cues.len(), 12);

        // running it again changes nothing
        assert_eq!(config.merge_starter(), 0);
        assert_eq!(config.cues.len(), 12);
    }

    #[test]
    fn merging_keeps_an_existing_cue_that_matches() {
        let mut config = Config::parse(
            std::path::Path::new("cues.toml"),
            "[[cue]]\ntext = \"WAREHOUSE\"\nat = \"09:00\"\n",
        )
        .unwrap();

        assert_eq!(config.merge_starter(), 11);
        assert_eq!(config.cues.len(), 12);
        assert_eq!(
            config.cues[0].text, "WAREHOUSE",
            "the existing cue stays first"
        );
    }

    #[test]
    fn ids_stay_unique_after_merging() {
        let mut config = Config::default();
        config.merge_starter();
        let mut ids: Vec<u64> = config.cues.iter().map(|c| c.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), config.cues.len());
    }
}

#[cfg(test)]
mod arrival_tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn a_fresh_install_has_a_card_due_within_two_minutes() {
        let now = Local.with_ymd_and_hms(2026, 3, 10, 14, 0, 0).unwrap();
        let config = Config::starter_at(now);
        let arrival = config.cues.last().unwrap();

        assert_eq!(arrival.text, "YOU ARE HERE");
        assert_eq!(arrival.repeat, Repeat::Once);
        let due = arrival
            .next_fire(now)
            .expect("the arrival cue is due today");
        let wait = due - now;
        assert!(
            wait > Duration::zero() && wait <= Duration::seconds(100),
            "{wait:?}"
        );
    }

    #[test]
    fn an_install_at_midnight_gets_examples_but_no_impossible_arrival() {
        let now = Local.with_ymd_and_hms(2026, 3, 10, 23, 59, 30).unwrap();
        let config = Config::starter_at(now);
        assert_eq!(
            config.cues.len(),
            12,
            "no arrival cue that would never fire"
        );
        assert!(config.cues.iter().all(|c| c.text != "YOU ARE HERE"));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn the_examples_button_does_not_add_another_arrival_cue() {
        let mut config = Config::starter_at(Local::now());
        let before = config.cues.len();
        config.merge_starter();
        assert_eq!(config.cues.len(), before);
    }
}

#[cfg(test)]
mod overlay_tests {
    use super::*;

    #[test]
    fn auto_is_permanent_only_on_multiple_screens() {
        assert!(!OverlayMode::Auto.permanent_on(1));
        assert!(OverlayMode::Auto.permanent_on(2));
        assert!(OverlayMode::Permanent.permanent_on(1));
        assert!(!OverlayMode::OnDemand.permanent_on(3));
    }

    #[test]
    fn the_mode_round_trips_and_auto_is_omitted() {
        let config = Config::parse(
            std::path::Path::new("cues.toml"),
            "[[cue]]\ntext = \"X\"\nat = \"18:00\"\n",
        )
        .unwrap();
        assert_eq!(config.overlay, OverlayMode::Auto);
        assert!(!config.to_toml().unwrap().contains("overlay ="));

        let mut with_mode = config.clone();
        with_mode.overlay = OverlayMode::OnDemand;
        let text = with_mode.to_toml().unwrap();
        assert!(text.contains("overlay = \"on_demand\""), "{text}");
        let again = Config::parse(std::path::Path::new("cues.toml"), &text).unwrap();
        assert_eq!(again.overlay, OverlayMode::OnDemand);
    }

    #[test]
    fn a_bad_mode_is_rejected() {
        let err = Config::parse(
            std::path::Path::new("cues.toml"),
            "overlay = \"sometimes\"\n[[cue]]\ntext = \"X\"\nat = \"18:00\"\n",
        )
        .unwrap_err();
        assert!(err.to_string().contains("overlay"), "{err}");
    }
}
