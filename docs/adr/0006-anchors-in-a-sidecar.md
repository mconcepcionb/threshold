# Anchors live beside the config, not inside it

Status: accepted

An interval cue counts from its last fire, and that instant has to survive a
restart. Storing it in `cues.toml` seemed convenient — the Cue already has an
`anchor` field — but it makes the app rewrite a file that a human is explicitly
allowed to edit, every twenty-five minutes.

Three things break when it does. The file's mtime becomes meaningless, so
"changed on disk" is a prompt that fires constantly and gets ignored. A scheduled
write can land on top of an unsaved editor buffer. And because a Cue's id is its
position in the array, an anchor write races any edit that reorders cues.

Anchors now live in `state.toml` next to the config, owned solely by the app, and
`cues.toml` is only written when the user saves from the settings window — or
when startup finds a file it cannot parse and replaces it with the starter set
(the unparseable file is kept as `cues.toml.broken-<timestamp>`). The sidecar is
keyed by what the cue *is* - text plus schedule - rather than its index, so
reordering or inserting cues keeps chains intact, while editing either field
starts the chain over. That is the honest reading of "you changed the cue".

The same sidecar pins a one-shot to the one occurrence it is armed for. Without
that, restarting the app would re-arm a shot whose time had passed. Re-enabling
a spent cue clears its entry and arms the next occurrence.

_Consequences_: deleting `state.toml` is safe; interval chains restart, spent
one-shots become armable again, and nothing in `cues.toml` is lost. Two files
describe one cue list, and the config remains the source of truth for everything
but the anchors.
