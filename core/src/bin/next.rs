//! Print what the scheduler would do with a config file, without a window.
//!
//!     cargo run --bin next -- ../examples/cues.toml
//!
//! Useful on its own while there is no app shell, and useful later as the
//! answer to "why didn't it fire?". Read-only: a mistyped path does not create
//! a config, and `state.toml` beside the file is read for the interval anchors.

use std::path::Path;
use std::process::ExitCode;

use chrono::Local;
use threshold_core::{Anchors, Config, Cue, LatePolicy, Repeat, Schedule};

fn main() -> ExitCode {
    let path = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("usage: next <cues.toml>");
            return ExitCode::FAILURE;
        }
    };

    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("could not read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let config = match Config::parse(Path::new(&path), &text) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    let anchors = Anchors::load(&Path::new(&path).with_file_name("state.toml"));
    let mut cues = config.cues.clone();
    anchors.apply(&mut cues);

    let now = Local::now();
    println!(
        "{} · floor {}ms · {} cues",
        path,
        config.floor_ms,
        cues.len()
    );
    println!("now {}\n", now.format("%Y-%m-%d %H:%M:%S %a"));

    let mut rows: Vec<(Option<chrono::DateTime<Local>>, &Cue)> =
        cues.iter().map(|c| (c.next_fire(now), c)).collect();
    rows.sort_by_key(|(at, _)| *at);

    for (at, cue) in rows {
        let when = match at {
            Some(t) => format!("{} (in {})", t.format("%a %H:%M"), humanize(t - now)),
            None if cue.is_one_shot() => "never - one-shot already fired".to_string(),
            None => "never".to_string(),
        };
        let state = if cue.enabled { "" } else { " · disarmed" };
        let schedule = describe(cue);
        println!("{when:<34} {:<22} {schedule}{state}", cue.text);
    }

    ExitCode::SUCCESS
}

fn describe(cue: &Cue) -> String {
    let at = match cue.schedule {
        Schedule::At(t) => t.format("%H:%M").to_string(),
        Schedule::Every(d) => format!("every {}m", d.num_minutes()),
    };
    let repeat = match &cue.repeat {
        Repeat::Once => String::new(),
        Repeat::Daily => " daily".into(),
        Repeat::Weekdays => " weekdays".into(),
        Repeat::Weekly(days) => format!(" {days:?}"),
    };
    let late = match cue.late {
        LatePolicy::Exact => String::new(),
        LatePolicy::CatchUp(d) => format!(" +{}m", d.num_minutes()),
        LatePolicy::NextOpportunity => " next-opportunity".into(),
    };
    format!("{at}{repeat}{late}")
}

fn humanize(d: chrono::Duration) -> String {
    let mins = d.num_minutes();
    if mins < 60 {
        format!("{mins}m")
    } else if mins < 60 * 24 {
        format!("{}h {:02}m", mins / 60, mins % 60)
    } else {
        format!("{}d {}h", mins / (60 * 24), (mins % (60 * 24)) / 60)
    }
}
