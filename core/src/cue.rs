//! The Cue: one scheduled moment, and the rules for when it actually fires.

use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, Weekday};

/// When a Cue is due.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Schedule {
    /// A wall-clock time, e.g. 18:00 local.
    At(NaiveTime),
    /// A rolling interval, e.g. every 25 minutes. Anchored to the last fire.
    Every(Duration),
}

/// How a wall-clock Cue repeats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repeat {
    Once,
    Daily,
    Weekdays,
    Weekly(Vec<Weekday>),
}

/// What to do with a Cue whose moment passed while nothing was running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatePolicy {
    /// Skip unless it fires within a couple of seconds of being due.
    Exact,
    /// Fire if still inside the grace window.
    CatchUp(Duration),
    /// Fire at the next opportunity, however late.
    NextOpportunity,
}

/// The outcome of asking whether a Cue should fire now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fire {
    OnTime,
    Late { by: Duration },
    Skip,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    pub id: u64,
    pub text: String,
    pub schedule: Schedule,
    pub repeat: Repeat,
    pub late: LatePolicy,
    /// The Window: total visible time for the Card, in seconds.
    pub seconds: u32,
    /// Semitone offset for the Sting, applied to the body only.
    pub sting: f32,
    pub enabled: bool,
    /// Interval chains persist this so "every 25m" survives a restart.
    pub anchor: Option<DateTime<Local>>,
}

impl Cue {
    pub fn at(text: impl Into<String>, time: NaiveTime) -> Self {
        Cue {
            id: 0,
            text: text.into(),
            schedule: Schedule::At(time),
            repeat: Repeat::Once,
            late: LatePolicy::Exact,
            seconds: 4,
            sting: 0.0,
            enabled: true,
            anchor: None,
        }
    }

    pub fn every(text: impl Into<String>, every: Duration) -> Self {
        Cue {
            schedule: Schedule::Every(every),
            repeat: Repeat::Once,
            ..Cue::at(text, NaiveTime::from_hms_opt(0, 0, 0).unwrap())
        }
    }

    /// True for a wall-clock cue that must consume exactly one occurrence.
    pub fn is_one_shot(&self) -> bool {
        matches!(self.schedule, Schedule::At(_)) && matches!(self.repeat, Repeat::Once)
    }

    /// The first moment this Cue is due strictly after `after`.
    ///
    /// Interval cues are anchored: the chain does not re-phase because the app
    /// was closed, it continues from where it stopped. A one-shot is anchored to
    /// the single occurrence it is armed for, so a restart cannot re-arm a shot
    /// that has already passed.
    pub fn next_fire(&self, after: DateTime<Local>) -> Option<DateTime<Local>> {
        match self.schedule {
            Schedule::Every(period) => {
                if period <= Duration::zero() {
                    return None;
                }
                let anchor = self.anchor.unwrap_or(after);
                let elapsed = (after - anchor).num_milliseconds();
                let step = period.num_milliseconds();
                let n = if elapsed < 0 { 1 } else { elapsed / step + 1 };
                let candidate =
                    anchor.checked_add_signed(Duration::milliseconds(n.checked_mul(step)?))?;

                if self.repeats_on(candidate.weekday()) {
                    return Some(candidate);
                }

                // A weekday-only interval has to skip the weekend, or Sunday
                // afternoon gets a "DEEP DIVE" card. Jump by whole days instead
                // of stepping the interval, so a one-second interval does not
                // have to walk two days of ticks to find Monday.
                let time = candidate.time();
                let mut date = candidate.date_naive();
                for _ in 0..8 {
                    date = date.succ_opt()?;
                    let Some(next) = local(date.and_time(time)) else {
                        continue;
                    };
                    if next > after && self.repeats_on(next.weekday()) {
                        return Some(next);
                    }
                }
                None
            }
            Schedule::At(time) => {
                // A one-shot is pinned to the occurrence it was armed for.
                if let Some(armed) = self.anchor {
                    return (armed > after).then_some(armed);
                }

                let mut date = after.date_naive();

                // Before it has been armed, a one-shot means the next occurrence
                // of its wall-clock time.
                if matches!(self.repeat, Repeat::Once) {
                    for _ in 0..=1 {
                        if let Some(candidate) = local(date.and_time(time)) {
                            if candidate > after {
                                return Some(candidate);
                            }
                        }
                        date = date.succ_opt()?;
                    }
                    return None;
                }

                // never scan more than a couple of weeks; a Weekly cue with no
                // listed days would otherwise loop forever. A wall-clock time
                // that does not exist (the spring-forward gap) is skipped rather
                // than aborting the whole search.
                for _ in 0..15 {
                    if let Some(candidate) = local(date.and_time(time)) {
                        if candidate > after && self.repeats_on(candidate.weekday()) {
                            return Some(candidate);
                        }
                    }
                    date = date.succ_opt()?;
                }
                None
            }
        }
    }

    fn repeats_on(&self, weekday: Weekday) -> bool {
        match &self.repeat {
            Repeat::Once | Repeat::Daily => true,
            Repeat::Weekdays => !matches!(weekday, Weekday::Sat | Weekday::Sun),
            Repeat::Weekly(days) => days.contains(&weekday),
        }
    }
}

fn local(naive: chrono::NaiveDateTime) -> Option<DateTime<Local>> {
    use chrono::TimeZone;
    // A DST fold can make a wall-clock time ambiguous; take the earliest.
    Local.from_local_datetime(&naive).earliest()
}

/// Decide what to do with a Cue that was due at `due`, now being `now`.
pub fn should_fire(policy: LatePolicy, due: DateTime<Local>, now: DateTime<Local>) -> Fire {
    let delta = now - due;
    if delta < Duration::zero() {
        return Fire::Skip;
    }
    let on_time = delta <= Duration::seconds(2);
    match policy {
        LatePolicy::Exact if on_time => Fire::OnTime,
        LatePolicy::Exact => Fire::Skip,
        LatePolicy::CatchUp(_) if on_time => Fire::OnTime,
        LatePolicy::CatchUp(grace) if delta <= grace => Fire::Late { by: delta },
        LatePolicy::CatchUp(_) => Fire::Skip,
        LatePolicy::NextOpportunity if on_time => Fire::OnTime,
        LatePolicy::NextOpportunity => Fire::Late { by: delta },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Timelike};

    fn local_at(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    fn hm(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn daily_cue_rolls_to_tomorrow_once_the_moment_passes() {
        let mut cue = Cue::at("GO DRINK WATER", hm(18, 0));
        cue.repeat = Repeat::Daily;

        let before = local_at(2026, 3, 10, 17, 0);
        let next = cue.next_fire(before).unwrap();
        assert_eq!((next.hour(), next.minute()), (18, 0));
        assert_eq!(next.day(), 10);

        let after = local_at(2026, 3, 10, 18, 0);
        let next = cue.next_fire(after).unwrap();
        assert_eq!(next.day(), 11);
    }

    #[test]
    fn weekday_cue_skips_the_weekend() {
        let mut cue = Cue::at("REVIEW MAIL", hm(9, 0));
        cue.repeat = Repeat::Weekdays;

        // Friday 2026-03-13, 10:00 — the next weekday is Monday the 16th.
        let friday = local_at(2026, 3, 13, 10, 0);
        assert_eq!(friday.weekday(), Weekday::Fri);
        let next = cue.next_fire(friday).unwrap();
        assert_eq!(next.weekday(), Weekday::Mon);
        assert_eq!(next.day(), 16);
    }

    #[test]
    fn weekly_cue_picks_a_listed_day() {
        let mut cue = Cue::at("STANDUP", hm(9, 30));
        cue.repeat = Repeat::Weekly(vec![Weekday::Tue, Weekday::Thu]);

        let monday = local_at(2026, 3, 9, 12, 0);
        let next = cue.next_fire(monday).unwrap();
        assert_eq!(next.weekday(), Weekday::Tue);
        assert_eq!((next.hour(), next.minute()), (9, 30));
    }

    #[test]
    fn a_one_shot_in_the_past_has_no_next_fire() {
        let mut cue = Cue::at("ONCE", hm(9, 0));
        cue.anchor = Some(local_at(2026, 3, 10, 9, 0));
        let after = local_at(2026, 3, 10, 10, 0);
        assert_eq!(cue.next_fire(after), None);
    }

    #[test]
    fn an_unarmed_one_shot_takes_the_next_occurrence() {
        let cue = Cue::at("ONCE", hm(9, 0));
        let after = local_at(2026, 3, 10, 10, 0);
        let next = cue.next_fire(after).unwrap();
        assert_eq!(
            next.day(),
            11,
            "10:00 has passed, so the shot is armed for tomorrow"
        );
        assert_eq!((next.hour(), next.minute()), (9, 0));
    }

    #[test]
    fn a_fired_one_shot_is_not_re_armed_by_a_restart() {
        let mut cue = Cue::at("ONCE", hm(9, 0));
        cue.anchor = Some(local_at(2026, 3, 10, 9, 0));
        // Still later the same day, and again a day later: the shot is spent.
        assert_eq!(cue.next_fire(local_at(2026, 3, 10, 10, 0)), None);
        assert_eq!(cue.next_fire(local_at(2026, 3, 11, 8, 0)), None);
    }

    #[test]
    fn intervals_are_never_one_shot() {
        assert!(!Cue::every("FOCUS", Duration::minutes(25)).is_one_shot());
        assert!(Cue::at("ONCE", hm(9, 0)).is_one_shot());
    }

    #[test]
    fn interval_cues_continue_from_their_anchor() {
        let anchor = local_at(2026, 3, 10, 9, 0);
        let mut cue = Cue::every("FOCUS", Duration::minutes(25));
        cue.anchor = Some(anchor);

        let next = cue.next_fire(local_at(2026, 3, 10, 9, 0)).unwrap();
        assert_eq!(next, anchor + Duration::minutes(25));

        // app closed for an hour: the chain continues, it does not re-phase
        let later = cue.next_fire(local_at(2026, 3, 10, 10, 0)).unwrap();
        assert_eq!(later, anchor + Duration::minutes(75));
    }

    #[test]
    fn a_weekday_interval_skips_the_weekend() {
        // Friday 23:00, every 25 minutes, weekdays only. The chain must not walk
        // into Saturday.
        let friday = local_at(2026, 3, 13, 23, 0);
        let mut cue = Cue::every("DEEP DIVE", Duration::minutes(25));
        cue.repeat = Repeat::Weekdays;
        cue.anchor = Some(friday);

        let next = cue.next_fire(friday).unwrap();
        assert_eq!(next, friday + Duration::minutes(25));

        // but once the chain reaches the weekend, it waits for Monday
        let after = local_at(2026, 3, 13, 23, 50);
        let next = cue.next_fire(after).unwrap();
        assert_eq!(next.weekday(), Weekday::Mon);
        assert_eq!(
            (next.hour(), next.minute()),
            (0, 15),
            "the jump keeps the chain's time of day instead of snapping to midnight"
        );
    }

    #[test]
    fn exact_policy_skips_when_late() {
        let due = local_at(2026, 3, 10, 18, 0);
        let now = due + Duration::seconds(30);
        assert_eq!(should_fire(LatePolicy::Exact, due, now), Fire::Skip);
        assert_eq!(
            should_fire(LatePolicy::Exact, due, due + Duration::seconds(1)),
            Fire::OnTime
        );
    }

    #[test]
    fn catch_up_fires_inside_its_grace_and_skips_outside() {
        let due = local_at(2026, 3, 10, 18, 0);
        let grace = LatePolicy::CatchUp(Duration::minutes(10));

        assert_eq!(
            should_fire(grace, due, due + Duration::minutes(9)),
            Fire::Late {
                by: Duration::minutes(9)
            }
        );
        assert_eq!(
            should_fire(grace, due, due + Duration::minutes(11)),
            Fire::Skip
        );
    }

    #[test]
    fn next_opportunity_fires_however_late() {
        let due = local_at(2026, 3, 10, 18, 0);
        let now = local_at(2026, 3, 11, 8, 0);
        assert!(matches!(
            should_fire(LatePolicy::NextOpportunity, due, now),
            Fire::Late { .. }
        ));
    }

    #[test]
    fn nothing_fires_before_it_is_due() {
        let due = local_at(2026, 3, 10, 18, 0);
        let now = due - Duration::minutes(1);
        assert_eq!(
            should_fire(LatePolicy::NextOpportunity, due, now),
            Fire::Skip
        );
    }
}
