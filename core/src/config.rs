//! The Config file: the Cue list as TOML.
//!
//! Two writers exist on purpose — the settings window and your text editor — so
//! this module owns the least-bad answer to that: the app always writes whole
//! files atomically, the file is fingerprinted so an external edit can be
//! detected before anything is overwritten, and a parse error never discards a
//! config that was loading fine a second ago.
//!
//! The on-disk form is deliberately forgiving: two required fields per Cue, five
//! optional, and defaults that mean a hand-written cue is two lines.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Duration, Local, NaiveTime};
use serde::{Deserialize, Serialize};

use crate::cue::{Cue, LatePolicy, Repeat, Schedule};

pub const FORMAT_VERSION: u32 = 1;
pub const DEFAULT_FLOOR_MS: u32 = 600;
pub const MAX_TEXT_CHARS: usize = 40;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub floor_ms: u32,
    /// Sting level, 0.0 to 1.0. One knob, because the sound is one sound.
    pub volume: f32,
    pub cues: Vec<Cue>,
}

impl Default for Config {
    fn default() -> Self {
        Config { floor_ms: DEFAULT_FLOOR_MS, volume: 1.0, cues: Vec::new() }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not read {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
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
            let present = self
                .cues
                .iter()
                .any(|c| {
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
        let file: ConfigFile = toml::from_str(text).map_err(|e| ConfigError::Toml {
            path: path.to_path_buf(),
            message: e.message().to_string(),
        })?;

        if file.version > FORMAT_VERSION {
            return Err(ConfigError::Version { found: file.version, ours: FORMAT_VERSION });
        }

        let mut cues = Vec::with_capacity(file.cues.len());
        for (i, raw) in file.cues.into_iter().enumerate() {
            let index = i + 1;
            cues.push(raw.into_cue(index).map_err(|message| ConfigError::Cue { index, message })?);
        }

        let config = Config { floor_ms: file.floor_ms, volume: file.volume, cues };
        config.validate()?;
        Ok(config)
    }

    pub fn to_toml(&self) -> Result<String, ConfigError> {
        self.validate()?;
        let file = ConfigFile {
            version: FORMAT_VERSION,
            floor_ms: self.floor_ms,
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
        for (i, cue) in self.cues.iter().enumerate() {
            let n = i + 1;
            let chars = cue.text.chars().count();
            if cue.text.trim().is_empty() {
                return Err(ConfigError::Cue { index: n, message: "text is empty".into() });
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
        }
        Ok(())
    }
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
    let (num, unit) = s.split_at(s.len().checked_sub(1).ok_or("empty duration")?);
    let n: i64 = num
        .parse()
        .map_err(|_| format!("duration `{s}` must look like 30s, 25m or 2h"))?;
    if n <= 0 {
        return Err(format!("duration `{s}` must be positive"));
    }
    match unit {
        "s" => Ok(Duration::seconds(n)),
        "m" => Ok(Duration::minutes(n)),
        "h" => Ok(Duration::hours(n)),
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
        Some(Stamp { modified: meta.modified().ok(), len: meta.len() })
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
    pub fn open_with_seed(
        path: impl Into<PathBuf>,
        seed: Config,
    ) -> Result<Store, ConfigError> {
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
            return Ok(Store { path, config, stamp });
        }

        let text = std::fs::read_to_string(&path)
            .map_err(|source| ConfigError::Io { path: path.clone(), source })?;
        let config = Config::parse(&path, &text)?;
        let stamp = Stamp::of(&path);
        Ok(Store { path, config, stamp })
    }

    pub fn path(&self) -> &Path {
        &self.path
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
        let text = std::fs::read_to_string(&self.path)
            .map_err(|source| ConfigError::Io { path: self.path.clone(), source })?;
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
    crate::fsutil::write_atomic(path, text)
        .map_err(|source| ConfigError::Io { path: path.to_path_buf(), source })
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
        Ok(Cue {
            id,
            text: self.text.trim().to_string(),
            schedule: parse_schedule(&self.at)?,
            repeat: if self.repeat.is_empty() { Repeat::Once } else { parse_repeat(&self.repeat)? },
            late: if self.late.is_empty() { LatePolicy::Exact } else { parse_late(&self.late)? },
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
    /// schedule is unchanged so an interval chain is not re-phased by an edit.
    pub fn from_drafts(&mut self, drafts: &[CueDraft]) -> Result<(), ConfigError> {
        let mut cues = Vec::with_capacity(drafts.len());
        for (i, draft) in drafts.iter().enumerate() {
            let index = i + 1;
            let mut cue = draft.to_cue(index as u64).map_err(|message| ConfigError::Cue {
                index,
                message,
            })?;
            cue.anchor = self
                .cues
                .get(i)
                .filter(|old| old.schedule == cue.schedule)
                .and_then(|old| old.anchor);
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
        assert_eq!(config.cues[0].schedule, Schedule::Every(Duration::minutes(25)));
        assert_eq!(config.cues[0].late, LatePolicy::CatchUp(Duration::minutes(10)));
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
        assert_eq!(config.cues[0].repeat, Repeat::Weekly(vec![Weekday::Tue, Weekday::Thu]));
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
            assert!(!out.contains(&format!("\n{field} =")), "{field} should be omitted:\n{out}");
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
        assert!(err.to_string().to_lowercase().contains("unknown field"), "{err}");
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
        assert_eq!(store.config().cues.len(), 12, "a fresh config starts from the starter file");

        let mut config = store.config().clone();
        config.cues.push(Cue::at("REVIEW MAIL", hm(18, 0)));
        store.save(config).unwrap();
        assert!(!store.changed_on_disk());

        let reopened = Store::open(&path).unwrap();
        assert_eq!(reopened.config().cues.len(), 13);
        assert_eq!(reopened.config().cues.last().unwrap().text, "REVIEW MAIL");

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn an_external_edit_is_noticed_but_a_failed_reload_keeps_the_old_config() {
        let path = temp_path("drift");
        let mut store = Store::open(&path).unwrap();
        let mut config = store.config().clone();
        config.cues.push(Cue::at("REVIEW MAIL", hm(18, 0)));
        store.save(config).unwrap();

        // someone else writes the file
        std::fs::write(&path, "[[cue]]\ntext = \"FROM EDITOR\"\nat = \"19:00\"\n").unwrap();
        assert!(store.changed_on_disk());

        // and then breaks it: the in-memory config must survive
        std::fs::write(&path, "this is not toml").unwrap();
        assert!(store.reload().is_err());
        assert_eq!(store.config().cues.last().unwrap().text, "REVIEW MAIL");

        std::fs::remove_file(&path).unwrap();
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
        assert_eq!(config.cues[0].text, "WAREHOUSE", "the existing cue stays first");
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
        let due = arrival.next_fire(now).expect("the arrival cue is due today");
        let wait = due - now;
        assert!(wait > Duration::zero() && wait <= Duration::seconds(100), "{wait:?}");
    }

    #[test]
    fn an_install_at_midnight_gets_examples_but_no_impossible_arrival() {
        let now = Local.with_ymd_and_hms(2026, 3, 10, 23, 59, 30).unwrap();
        let config = Config::starter_at(now);
        assert_eq!(config.cues.len(), 12, "no arrival cue that would never fire");
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
