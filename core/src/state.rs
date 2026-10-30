//! Anchors: the only part of a Cue that changes while the app runs.
//!
//! An interval cue counts from its last fire, and that instant has to survive a
//! restart. Keeping it in `cues.toml` would mean the app rewrites a file a human
//! is allowed to edit every twenty-five minutes — which makes the file's mtime
//! meaningless, makes "changed on disk" prompt noise, and lets a scheduled write
//! land on top of an unsaved editor buffer. So the anchors live beside the config
//! in `state.toml`, keyed by what the cue *is* rather than its position: the text
//! and the schedule. Edit either and the chain starts over, which is the honest
//! reading of "you changed the cue".
//!
//! See ADR-0006.

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::cue::{Cue, Schedule};
use crate::fsutil::write_atomic;

#[derive(Debug, Default, Serialize, Deserialize)]
struct StateFile {
    #[serde(default, rename = "anchor")]
    anchors: Vec<Entry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Entry {
    cue: String,
    since: String,
}

#[derive(Debug, Default)]
pub struct Anchors {
    keyed: HashMap<String, DateTime<Local>>,
}

impl Anchors {
    /// What makes two versions of a cue "the same cue": its text and its timing.
    pub fn key(cue: &Cue) -> String {
        format!("{}|{}", cue.text, schedule_key(&cue.schedule))
    }

    pub fn load(path: &Path) -> Anchors {
        let Ok(text) = std::fs::read_to_string(path) else { return Anchors::default() };
        let Ok(file) = toml::from_str::<StateFile>(&text) else { return Anchors::default() };

        let mut keyed = HashMap::new();
        for entry in file.anchors {
            if let Ok(at) = DateTime::parse_from_rfc3339(&entry.since) {
                keyed.insert(entry.cue, at.with_timezone(&Local));
            }
        }
        Anchors { keyed }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut entries: Vec<Entry> = self
            .keyed
            .iter()
            .map(|(cue, at)| Entry { cue: cue.clone(), since: at.to_rfc3339() })
            .collect();
        entries.sort_by(|a, b| a.cue.cmp(&b.cue));

        let body = toml::to_string(&StateFile { anchors: entries }).map_err(std::io::Error::other)?;
        let header = "# Written by Threshold. Safe to delete; interval cues restart.\n\n";
        write_atomic(path, &format!("{header}{body}"))
    }

    pub fn set(&mut self, cue: &Cue, at: DateTime<Local>) {
        self.keyed.insert(Self::key(cue), at);
    }

    pub fn get(&self, cue: &Cue) -> Option<DateTime<Local>> {
        self.keyed.get(&Self::key(cue)).copied()
    }

    /// Stamp the loaded anchors onto a cue list. Nothing is written.
    pub fn apply(&self, cues: &mut [Cue]) {
        for cue in cues.iter_mut() {
            if let Schedule::Every(_) = cue.schedule {
                cue.anchor = self.get(cue);
            }
        }
    }
}

fn schedule_key(schedule: &Schedule) -> String {
    match schedule {
        Schedule::At(t) => t.format("%H:%M").to_string(),
        Schedule::Every(d) => format!("every{}", d.num_milliseconds()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::cue::Repeat;
    use chrono::{Duration, TimeZone};

    fn path(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("threshold-state-{name}-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn anchors_round_trip_through_the_sidecar() {
        let p = path("rt");
        let cue = Cue::every("FOCUS", Duration::minutes(25));
        let at = Local.with_ymd_and_hms(2026, 3, 10, 9, 0, 0).unwrap();

        let mut anchors = Anchors::default();
        anchors.set(&cue, at);
        anchors.save(&p).unwrap();

        let loaded = Anchors::load(&p);
        assert!(loaded.get(&cue).is_some());
        assert_eq!(loaded.get(&cue).unwrap(), at);
        std::fs::remove_file(&p).unwrap();
    }

    #[test]
    fn editing_a_cue_starts_its_chain_over() {
        let cue = Cue::every("FOCUS", Duration::minutes(25));
        let mut anchors = Anchors::default();
        anchors.set(&cue, Local::now());

        let mut renamed = cue.clone();
        renamed.text = "DEEP WORK".into();
        assert!(anchors.get(&renamed).is_none());

        let mut retimed = cue.clone();
        retimed.schedule = Schedule::Every(Duration::minutes(50));
        assert!(anchors.get(&retimed).is_none());
    }

    #[test]
    fn applying_anchors_touches_only_interval_cues() {
        let p = path("apply");
        std::fs::write(
            &p,
            "[[anchor]]\ncue = \"FOCUS|every1500000\"\nsince = \"2026-03-10T09:00:00+00:00\"\n",
        )
        .unwrap();

        let mut config: Config = Config::parse(
            std::path::Path::new("cues.toml"),
            "[[cue]]\ntext = \"FOCUS\"\nat = \"every 25m\"\n\n[[cue]]\ntext = \"MAIL\"\nat = \"18:00\"\n",
        )
        .unwrap();

        Anchors::load(&p).apply(&mut config.cues);
        assert!(config.cues[0].anchor.is_some());
        assert!(config.cues[1].anchor.is_none());
        std::fs::remove_file(&p).unwrap();
    }

    #[test]
    fn a_repeat_rule_does_not_change_the_anchors_identity() {
        let mut a = Cue::every("FOCUS", Duration::minutes(25));
        let b = a.clone();
        a.repeat = Repeat::Weekdays;
        let mut anchors = Anchors::default();
        anchors.set(&b, Local::now());
        assert!(anchors.get(&a).is_some());
    }
}
