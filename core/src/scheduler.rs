//! The tick decision, separated from windows and audio so it can be tested.
//!
//! The shell owns a map of "when each cue is next due" and calls `plan_tick`
//! every quarter second. Everything that decides *whether* a card fires lives
//! here; the shell only shows the result and persists the anchors.

use std::collections::HashMap;

use chrono::{DateTime, Local};

use crate::cue::{should_fire, Cue, Fire, Schedule};

/// What one tick should do. `next` is the map to store after the tick.
#[derive(Debug, Default)]
pub struct TickPlan {
    /// Cues that should produce a card now, in config order.
    pub fire: Vec<Cue>,
    /// Interval chains that fired. Their due time becomes the new anchor, so a
    /// slow tick cannot drift the phase.
    pub anchors: Vec<(Cue, DateTime<Local>)>,
    pub next: HashMap<u64, Option<DateTime<Local>>>,
}

/// Decide one tick.
///
/// `suppressed` is the emergency stop (pause or escape hatch): due cues are
/// advanced past, never fired, so resuming does not replay the pause.
pub fn plan_tick(
    cues: &[Cue],
    next: &HashMap<u64, Option<DateTime<Local>>>,
    now: DateTime<Local>,
    suppressed: bool,
) -> TickPlan {
    let mut plan = TickPlan {
        next: next.clone(),
        ..TickPlan::default()
    };

    for cue in cues.iter().filter(|c| c.enabled) {
        let Some(due) = next.get(&cue.id).copied().flatten() else {
            continue;
        };
        if due > now {
            continue;
        }

        let fire = !suppressed && !matches!(should_fire(cue.late, due, now), Fire::Skip);
        if fire {
            plan.fire.push(cue.clone());
            // a one-shot is spent by its anchor: next_fire returns None from now
            // on, and the config file is never rewritten to disable it.
            if matches!(cue.schedule, Schedule::Every(_)) {
                plan.anchors.push((cue.clone(), due));
            }
        }
        plan.next.insert(cue.id, cue.next_fire(now));
    }

    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cue::{LatePolicy, Repeat};
    use chrono::{Duration, NaiveTime, TimeZone};

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    fn hm(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn next_of(cue: &Cue, now: DateTime<Local>) -> HashMap<u64, Option<DateTime<Local>>> {
        let mut map = HashMap::new();
        map.insert(cue.id, cue.next_fire(now));
        map
    }

    #[test]
    fn an_interval_fires_and_stays_armed() {
        let mut cue = Cue::every("FOCUS", Duration::minutes(25));
        cue.id = 1;
        cue.anchor = Some(at(2026, 3, 10, 9, 0));
        let due = cue.next_fire(at(2026, 3, 10, 9, 0)).unwrap();
        let plan = plan_tick(
            &[cue.clone()],
            &next_of(&cue, at(2026, 3, 10, 9, 0)),
            due,
            false,
        );

        assert_eq!(plan.fire.len(), 1, "the interval must fire");
        assert_eq!(plan.anchors.len(), 1, "the chain must be re-anchored");
        assert_eq!(
            plan.next[&1],
            Some(due + Duration::minutes(25)),
            "and it must remain armed for the next interval"
        );
    }

    #[test]
    fn a_one_shot_fires_once_and_is_then_spent() {
        let mut cue = Cue::at("ONCE", hm(9, 0));
        cue.id = 1;
        cue.anchor = Some(at(2026, 3, 10, 9, 0));
        let first = plan_tick(
            &[cue.clone()],
            &next_of(&cue, at(2026, 3, 10, 8, 0)),
            at(2026, 3, 10, 9, 0),
            false,
        );
        assert_eq!(first.fire.len(), 1);
        assert_eq!(first.next[&1], None);

        let second = plan_tick(&[cue], &first.next, at(2026, 3, 11, 8, 0), false);
        assert!(second.fire.is_empty(), "a spent one-shot must not re-arm");
    }

    #[test]
    fn pause_skips_due_cues_without_replaying_them() {
        let mut cue = Cue::every("FOCUS", Duration::minutes(25));
        cue.id = 1;
        cue.anchor = Some(at(2026, 3, 10, 9, 0));
        let due = cue.next_fire(at(2026, 3, 10, 9, 0)).unwrap();
        let plan = plan_tick(
            &[cue.clone()],
            &next_of(&cue, at(2026, 3, 10, 9, 0)),
            due,
            true,
        );

        assert!(plan.fire.is_empty());
        assert!(plan.anchors.is_empty());
        assert!(
            plan.next[&1].is_some_and(|n| n > due),
            "the cue is advanced past the pause"
        );
    }

    #[test]
    fn exact_late_cues_are_skipped_but_advanced() {
        let mut cue = Cue::at("MAIL", hm(18, 0));
        cue.id = 1;
        cue.repeat = Repeat::Daily;
        let due = at(2026, 3, 10, 18, 0);
        let mut map = HashMap::new();
        map.insert(1, Some(due));
        let plan = plan_tick(&[cue], &map, due + Duration::minutes(5), false);

        assert!(plan.fire.is_empty(), "exact does not catch up");
        assert_eq!(plan.next[&1], Some(at(2026, 3, 11, 18, 0)));
    }

    #[test]
    fn catch_up_fires_inside_the_grace_window() {
        let mut cue = Cue::at("MAIL", hm(18, 0));
        cue.id = 1;
        cue.repeat = Repeat::Daily;
        cue.late = LatePolicy::CatchUp(Duration::minutes(10));
        let due = at(2026, 3, 10, 18, 0);
        let mut map = HashMap::new();
        map.insert(1, Some(due));
        let plan = plan_tick(&[cue], &map, due + Duration::minutes(5), false);

        assert_eq!(plan.fire.len(), 1);
    }
}
