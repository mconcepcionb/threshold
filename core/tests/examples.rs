//! The shipped starter config has to parse, and every enabled cue has to be
//! able to fire from some point in the future. These files are read by people
//! before they are read by the app, so a broken example is a broken first
//! impression.

use std::path::Path;

use chrono::{Duration, Local, TimeZone};
use threshold_core::{Config, Cue, Font, WrapSpec};

fn starter() -> Config {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/cues.toml");
    let text = std::fs::read_to_string(&path).expect("examples/cues.toml exists");
    Config::parse(&path, &text).expect("the starter config parses")
}

#[test]
fn the_starter_config_parses_and_has_twelve_cues() {
    let config = starter();
    assert_eq!(config.cues.len(), 12);
    assert_eq!(config.floor_ms, 600);
}

#[test]
fn every_text_fits_a_card() {
    for cue in &starter().cues {
        let chars = cue.text.chars().count();
        assert!(
            (3..=40).contains(&chars),
            "{:?} is {chars} characters",
            cue.text
        );
    }
}

#[test]
fn every_enabled_cue_has_a_next_fire() {
    // a Monday morning, so weekday and weekly cues all have somewhere to land
    let monday = Local.with_ymd_and_hms(2026, 3, 9, 7, 0, 0).unwrap();
    let mut checked = 0;
    for cue in starter().cues.iter().filter(|c: &&Cue| c.enabled) {
        match cue.next_fire(monday) {
            Some(at) => {
                assert!(at > monday, "{:?} fired in the past", cue.text);
                assert!(
                    at - monday < Duration::days(8),
                    "{:?} is more than a week out",
                    cue.text
                );
                checked += 1;
            }
            None => panic!("{:?} can never fire", cue.text),
        }
    }
    assert_eq!(
        checked, 11,
        "one cue in the starter file is disarm(ed) on purpose"
    );
}

#[test]
fn interval_cues_get_a_card_and_a_thump_too() {
    let config = starter();
    let interval = config
        .cues
        .iter()
        .find(|c| matches!(c.schedule, threshold_core::Schedule::Every(_)))
        .expect("the starter file has interval cues");
    assert!(interval.seconds >= 1);
    assert!(interval.sting.abs() <= 6.0);
    assert!(!interval.is_one_shot(), "an interval must keep firing");
}

#[test]
fn every_starter_text_fits_a_1920_frame() {
    let font = Font::from_bytes(
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("assets/texgyreadventor-bold.otf"),
        )
        .expect("the bundled font is readable"),
    )
    .expect("the bundled font parses");
    for cue in &starter().cues {
        let layout =
            threshold_core::layout::wrap(&font, &cue.text.to_uppercase(), 1920.0, &WrapSpec::hd());
        for line in &layout.lines {
            let width = font.width(line, layout.size_px, WrapSpec::hd().tracking_em);
            assert!(
                width <= 1920.0 * WrapSpec::hd().max_width_fraction + 1.0,
                "{:?} line {line:?} is {width}px wide",
                cue.text
            );
        }
    }
}
