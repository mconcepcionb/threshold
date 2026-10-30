//! Threshold's domain: when a Cue fires, and what a Card does with its text.
//!
//! Everything here is headless and deterministic. The app shell owns windows,
//! audio devices and the tray; this crate owns the two things that can be wrong
//! in a way the user notices: firing at the wrong moment, and setting type that
//! overflows.

pub mod config;
pub mod fsutil;
pub mod cue;
pub mod layout;
pub mod state;

pub use config::{Config, ConfigError, CueDraft, OverlayMode, Store, DEFAULT_FLOOR_MS};
pub use cue::{should_fire, Cue, Fire, LatePolicy, Repeat, Schedule};
pub use layout::{Font, Layout, WrapSpec};
pub use state::Anchors;
