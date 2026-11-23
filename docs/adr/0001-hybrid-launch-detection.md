# Wrapped launches where possible, unwrapped detection and Background Pull elsewhere

A real Pre-launch Sync needs RommSaveSyncer to run before the emulator reads its Save, which only a Wrapped Launch guarantees. ES-DE (Windows) and EmuDeck's Steam ROM Manager and ES-DE setup (Bazzite) allow a wrapper command, so RommSaveSyncer wraps launches there. NeoStation on Android has no custom launch command, so on the AYN Thor every launch is an Unwrapped Launch. There, Background Pull keeps Saves current ahead of time, and RommSaveSyncer warns on the bottom screen when RomM holds a newer Save. RommSaveSyncer replaces NeoStation's own RomM save sync (which the user turns off) so that a single engine applies one set of Conflict rules.

## Considered Options

- Let NeoStation keep syncing on the Thor: rejected, because two sync engines would write the same Saves with different Conflict behaviour.
- Switch the Thor's launcher to ES-DE: rejected, because the user keeps NeoStation.
- Contribute a custom launch command to NeoStation (issue #501): kept as an optional later milestone that would turn Thor launches into Wrapped Launches.

## Consequences

- On the Bazzite, EmuDeck overwrites its own launcher scripts on every launch, so the wrapper must be a separately named script that calls EmuDeck's script, and never an edit of it.
- Wrapper changes to launcher settings are backed up, can be undone, and are re-applied ("repair") after launcher updates.
