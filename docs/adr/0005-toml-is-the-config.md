# TOML is the config, and the app does not own it

Status: accepted

The Cue list lives in a TOML file in the user's config directory. The settings
window and a text editor are both first-class ways to edit it, with the same
expressiveness; neither is a fallback for the other.

The alternative — a settings window writing to a private store, with import and
export — was rejected because the app's only screen is the Card, and a personal
tool whose entire configuration is one readable file is easier to trust, diff and
back up than a database you cannot see. It also makes the domain model legible in
the README, which is worth something in a project like this.

Two writers is the cost, and it is paid in three places: writes are atomic
(temp file, then rename) so a crash cannot leave half a config; the file is
fingerprinted by mtime and size so the app can notice an external edit before it
overwrites one; and a parse error never discards the config that was loading
fine — the last good one stays in memory and the error is surfaced instead.

_Consequences_: unknown keys are an error rather than silently ignored, because a
typo in a hand-edited file should be loud. Defaults are omitted when writing, so
the file a person reads stays as short as the file they would have typed.
