# Security

Threshold is a local desktop app. It has no server, no account, no telemetry and
no network requests of its own. It reads and writes two files in your config
directory and, when a cue fires, draws a window and plays a synthesized sound.

## Reporting a vulnerability

Please do not open a public issue for a vulnerability. Use GitHub's private
vulnerability reporting on
[github.com/mconcepcionb/threshold/security/advisories/new](https://github.com/mconcepcionb/threshold/security/advisories/new).

Include the version, the platform, and the smallest reproduction you can. A
fix and a coordinated disclosure usually follow within a few days for a project
this size.

## What the crash reporter writes

A Rust panic writes `crash-<timestamp>.log` beside `cues.toml` containing the
app version, the OS name, the time, and the panic message and location. It does
**not** include cue text, the config, or anything from your desktop. Nothing is
uploaded. The tray's **Report a problem** opens a prefilled GitHub issue; you
choose whether to submit it.

## Supported versions

Only the latest release receives fixes. The app is an alpha; backports are not
provided.
