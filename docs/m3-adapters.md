# M3 multi-system emulator adapters

**Specification:** 1.2  
**Status:** Implementation committed on 2026-10-23; Bazzite target qualification pending. No configuration is enabled for automatic writes.  
**Target:** Bazzite/Linux

## Boundary

`romm-sync-adapters` turns one game's emulator-specific save location into the M1B discovery and execution vocabulary. Adapters list directories and read save bytes; they never write. Every mutation still goes through the guarded M1B executor, and every reconciliation decision still comes from the pure M1A planner.

An adapter resolution supplies:

- the save root and the exact adapter-owned member files;
- the local-to-RomM member mapping and the upload filename;
- the RomM emulator metadata;
- the planner `AdapterSnapshot`, with compatibility lowered by any structural block;
- format validation for local members and downloaded payloads; and
- stable `ReadinessReason` codes and diagnostics explaining every block.

`AdapterResolution::observe` runs M1B discovery (two hashed observations 250 ms apart), re-reads each owned member, confirms its SHA-256 matches the hashed snapshot, and validates its format. A mismatch becomes `incomplete`; a format failure becomes `malformed`. Both are non-executable.

The executor gained one hook: `ExecutionHook::validate_payload`. It runs immediately after the transport returns a download and before anything is staged. A payload that fails adapter validation is rejected with `payload_invalid` and the live save is untouched. The default implementation accepts every payload, so M1B/M2 behavior is unchanged for existing callers.

## Readiness model

A configuration's stage is a reviewed source-code constant, never a runtime setting, so configuration files cannot accidentally enable writes.

| Stage | Planner status | Meaning |
|---|---|---|
| `planned` | pending validation | Represented; installation, root or save model unverified |
| `discovery_ready` | pending validation | Read-only discovery understood well enough to prototype |
| `fixture_validated` | pending validation | Synthetic fixtures pass in CI; eligible for disposable qualification only |
| `target_validated` | **validated** | Approved round trips passed on the Beelink; the only stage that permits automatic writes |
| `unsupported` | unsupported | Known unsafe configuration; never written |

`SnapshotMode::Qualification` presents a fixture-validated adapter as validated under a distinct `<id>+qualification` adapter identity. Only `romm-m3-qualify` uses it, and only on copies in a new workspace.

`romm-sync adapters` prints the full inventory as JSON.

## Compatibility matrix

| Platform | Configuration | Family | Save model | Stage | Blocking reasons |
|---|---|---|---|---|---|
| nds | `retroarch-melondsds-nds` — RetroArch melonDS DS 1.4.0 | RetroArch | single file | fixture validated | `awaiting_target_qualification` |
| gba | `retroarch-mgba-gba` — RetroArch mGBA 0.10-dev | RetroArch | single file | fixture validated | `awaiting_target_qualification`, `awaiting_legal_fixture` |
| ngc | `dolphin-gci-folder-ngc` — Dolphin 2609a, Slot A GCI Folder, Slot B None | Dolphin standalone | single file | fixture validated | `awaiting_target_qualification`, `multi_file_transport_unsupported` |
| nes | `retroarch-mesen-nes` | RetroArch | single file | planned | `emulator_unverified`, `save_root_unverified` |
| snes | `retroarch-snes9x-snes` | RetroArch | single file | planned | `emulator_unverified`, `save_root_unverified`, `save_model_unverified` |
| psx | `retroarch-beetle-psx-per-game-card-psx` | RetroArch | game-specific card | planned | `emulator_unverified`, `save_model_unverified`, `identity_rules_unverified` |
| psx | `retroarch-beetle-psx-shared-card-psx` | RetroArch | shared card | unsupported | `shared_memory_card` |
| n64 | `retroarch-mupen64plus-next-n64` | RetroArch | multi file | planned | `emulator_unverified`, `save_model_unverified`, `multi_file_transport_unsupported` |
| psp | `retroarch-ppsspp-psp` | RetroArch | directory tree | planned | `emulator_unverified`, `save_root_unverified`, `multi_file_transport_unsupported` |
| wii | `dolphin-wii-title-tree-wii` | Dolphin standalone | directory tree | planned | `save_model_unverified`, `identity_rules_unverified`, `multi_file_transport_unsupported` |
| 3ds | `retroarch-citra-3ds` (core unresolved) | RetroArch | directory tree | planned | `emulator_unverified`, `save_root_unverified`, `identity_rules_unverified`, `multi_file_transport_unsupported` |
| ps2 | `retroarch-pcee2-folder-card-ps2` | RetroArch | directory tree | planned | `emulator_unverified`, `save_model_unverified`, `identity_rules_unverified`, `multi_file_transport_unsupported` |
| ps2 | `retroarch-pcee2-shared-card-ps2` | RetroArch | shared card | unsupported | `shared_memory_card` |
| switch | `ryujinx-user-save-switch` — Ryujinx (Standalone) | Ryujinx standalone | directory tree | planned | `emulator_unverified`, `save_model_unverified`, `identity_rules_unverified`, `multi_file_transport_unsupported` |

Planned and unsupported rows declare no transfer direction at all. Twelve-platform compatibility is not claimed: three configurations are fixture-validated and none is target-validated.

### Verified in actual use

| Configuration | Emulator/config version | Bazzite qualification | Date |
|---|---|---|---|
| `retroarch-melondsds-nds` | melonDS DS 1.4.0 | Pending `romm-m3-qualify battery nds` | — |
| `retroarch-mgba-gba` | mGBA 0.10-dev | Pending legal fixture and `romm-m3-qualify battery gba` | — |
| `dolphin-gci-folder-ngc` | Dolphin 2609a | Pending `romm-m3-qualify gci` (Kirby Air Ride) | — |

M2's 2026-10-15 DS run qualified the RomM transport with a DS save; it predates the adapter and does not count as adapter qualification.

## RetroArch battery adapter

`RetroArch` writes `<content-basename>.srm` into the effective save root. The audited configuration disables core and content sorting, so the root is the global save directory. Same-stem files are classified:

| Suffix | Treatment |
|---|---|
| `srm` | The owned save |
| `state`, `state1`, `state.auto`, `state1.png`, … | Save state: excluded (`save_state_excluded`) |
| `sav` (DS only) | The manual M0 test artifact: excluded (`excluded_test_artifact`) |
| a longer dotted suffix such as `Game.v2.srm` | Another content's file (`other_content_ignored`) |
| `rtc` | Known companion: the group becomes multi-file and blocks (`companion_file_unsupported`) |
| anything else | Unknown companion: compatibility becomes unknown and the save defers (`unrecognized_companion`) |

The RomM 5.4 transfer boundary accepts one file per operation, so any companion blocks rather than being silently dropped. The `.srm` is mapped to the RomM head's filename when it uses `.sav`/`.srm`, otherwise to `<stem>.sav`.

Size rules reject impossible battery saves before planning and before staging a download:

| Platform | Accepted sizes |
|---|---|
| DS | power of two, 512 B – 32 MiB |
| GBA | 512 B, 8 KiB, 32 KiB, 64 KiB, 128 KiB |
| NES (planned) | power of two, 128 B – 1 MiB |
| SNES (planned) | power of two, 2 KiB – 512 KiB |

A GBA save with an appended RTC footer is therefore rejected (`battery_size_unrecognized`) until an mGBA RTC fixture proves its layout.

## Dolphin GameCube GCI adapter

The adapter reads `<GC root>/<REGION>/Card A`, where the region comes from the fourth byte of the six-byte disc game ID (`E` → `USA`, `J` → `JAP`, PAL countries → `EUR`). Unknown countries, including NTSC-K, block (`gci_region_unrecognized`). Any Dolphin card configuration other than the audited Slot A GCI Folder / Slot B None blocks (`dolphin_slot_configuration_unsupported`).

Every `.gci` header is parsed: four-byte game code and two-byte maker code (uppercase ASCII alphanumeric), a block count of 1–2043, and a length exactly equal to `0x40 + blocks × 0x2000`. Ownership comes from the header matching the full game ID, never from the filename.

- Exactly one owned GCI is a compatible single-file save.
- Two or more owned GCIs are detected as a multi-file group and block (`multi_gci_group`, `multi_file_transport_unsupported`).
- Any unparseable `.gci` in the folder makes ownership unprovable; the save is malformed (`gci_malformed`).
- The same title from another region in the folder is ignored (`same_title_other_region_gci_ignored`).
- Non-GCI entries such as the M0 `.raw` file are ignored (`non_gci_entry_ignored`).
- A downloaded payload must parse and be owned by the same game ID; a PAL payload for an NTSC game, a truncated GCI or another game's GCI is rejected before staging.
- When no local GCI exists, the RomM head's filename is used for the download if it is a safe `.gci` name; otherwise `<maker>-<game>-romm-autosync.gci`.

Only `.gci` remote heads are accepted. A ZIP head blocks (`remote_format_unsupported`) instead of the adapter falling back to an older GCI version; superseded ZIPs elsewhere in history are ignored because only the head is considered.

## ROM identity, region, revision and multi-disc rules

Filename tags never establish identity. `assess_identity` requires a unique hash match against one RomM ROM and then checks the local and RomM filenames:

- different titles → `rom_title_mismatch`;
- different region tags when both names have them → `rom_region_mismatch`;
- different `Rev N` / `vX.Y` tags (absence counts as the original revision) → `rom_revision_mismatch`;
- no unique hash match → `hash_identity_unverified`.

Any of these makes identity ambiguous, which the planner defers. Language tags and disc numbers are ignored.

`retroarch_content_stem` picks the battery-save owner for RetroArch content. A single file uses its stem. Multi-disc content must be launched from exactly one `.m3u` so every disc shares one save; discs without a playlist (`multi_disc_without_playlist`), discs with mismatched title/region/revision, or unrelated files (`ambiguous_content`) block. GameCube discs of one title share a game ID, so both discs resolve to the same GCI owner.

## Fixtures

All fixtures are synthetic bytes generated in temporary directories; no save binary, ROM or BIOS is committed. `crates/romm-sync-adapters/tests/m3_fixtures.rs` covers, for each adapter class:

| Case | Battery (DS/GBA) | Dolphin GCI |
|---|---|---|
| Normal | single `.srm`, production stays disabled, qualification plans upload | header-owned GCI beside a foreign GCI and `.raw` |
| Missing | absent save downloads into `<stem>.srm` | absent save downloads into the safe remote name or placeholder |
| Malformed | 1000-byte GBA save | truncated GCI in the folder; bad length, zero blocks, lowercase code |
| Conflicting | baseline-free and both-changed conflicts | baseline-free conflict |
| Multi-file | `.rtc` companion blocks; unknown companion defers | two owned GCIs block |
| Region/revision | — | PAL GCI in the USA folder, NTSC-K block, PAL payload for NTSC game |
| Remote head | ZIP head and impossible size block | ZIP head blocks |
| Executor | guarded upload leaves the local save untouched | guarded download backs up and replaces; foreign or truncated payload rejected before staging |

Identity tests cover region, revision, title and hash mismatches, multi-disc playlists, missing playlists and mixed-region discs. CLI tests prove `adapters`, `inspect-battery` and `inspect-gci` are read-only.

## Bazzite target qualification

`romm-m3-qualify` is a temporary Bazzite/Linux-only tool. It reuses the credential paired with `romm-m2-qualify`. Successful pushes to `main` publish `romm-m3-qualify-linux-x86_64.tar.gz` for 14 days; it contains `romm-m3-qualify`, the read-only `romm-sync` CLI, `BUILD-INFO.txt` and `SHA256SUMS`.

```bash
QUAL_DIR="$HOME/Applications/romm-m3-qualify-$(date +%Y%m%d-%H%M%S)"
mkdir -m 700 "$QUAL_DIR"
tar -xzf "$HOME/Downloads/romm-m3-qualify-linux-x86_64.tar.gz" -C "$QUAL_DIR"
(
  cd "$QUAL_DIR"
  sha256sum -c SHA256SUMS
  cat BUILD-INFO.txt
  ./romm-m3-qualify --help
)
QUAL_BIN="$QUAL_DIR/romm-m3-qualify"
SYNC_BIN="$QUAL_DIR/romm-sync"
REAL_HOME="/var/home/$USER"
```

Inspect first. These commands are read-only and do not contact RomM:

```bash
"$SYNC_BIN" adapters
"$SYNC_BIN" inspect-battery nds "$REAL_HOME/Emulation/saves/retroarch/saves" "CONTENT_STEM"
"$SYNC_BIN" inspect-gci "$REAL_HOME/.var/app/org.DolphinEmu.dolphin-emu/data/dolphin-emu/GC" GAME_ID
```

`CONTENT_STEM` is the ROM filename without its extension (the `.srm` name without `.srm`). `GAME_ID` is the six-character ID from Dolphin's game properties (for example `GKYE01`); take it from Dolphin rather than from this document. The inspection must report `local_state: present`, one member and no `reasons`.

Close the emulator, then run one qualification per configuration with a new workspace outside the save tree:

```bash
"$QUAL_BIN" battery nds "$REAL_HOME/.config/romm-save-sync/credentials.json" ROM_ID \
  "$REAL_HOME/Emulation/saves/retroarch/saves" "CONTENT_STEM" \
  "$REAL_HOME/romm-m3-nds-qualification" \
  --allow-insecure-http --confirm-disposable-romm-mutation

"$QUAL_BIN" gci "$REAL_HOME/.config/romm-save-sync/credentials.json" ROM_ID \
  "$REAL_HOME/.var/app/org.DolphinEmu.dolphin-emu/data/dolphin-emu/GC" GAME_ID \
  "$REAL_HOME/romm-m3-gci-qualification" \
  --allow-insecure-http --confirm-disposable-romm-mutation
```

`ROM_ID` must be the hash-verified RomM ROM for the save. With an existing remote head (the default), the tool:

1. validates and copies the live save through its adapter, and validates the RomM head's format and bytes;
2. proves the pure planner classifies the baseline-free divergence as a conflict;
3. runs a guarded **download** through the M1B executor and `RomMTransport` into a workspace copy — adapter payload validation, backup, atomic replacement, read-back, then the `/downloaded` acknowledgement;
4. runs a guarded **upload** of the copied local bytes through the executor, verifies the head MD5 and proves a repeated upload is deduplicated;
5. restores the original remote head and checks that exactly two upload POSTs occurred; and
6. verifies the live save is byte-identical.

It refuses slots with more than seven retained versions, a non-matching head format, or identical local and remote bytes. For an empty slot (the likely GBA case), add `--confirm-initial-upload`: the tool plans and executes an initial upload, deduplicates a repeat, then plans and executes an initial download into an empty workspace folder and checks the bytes are identical. That mode leaves one retained RomM version (the copied save) and performs exactly one upload POST.

GBA requires an approved disposable or homebrew ROM and an mGBA-generated save first; M0 found none on the Beelink.

The output is a sanitized JSON report with configuration, revision, mode, RomM version, slot, extension, size, check names, version counts and POST count. It contains no server address, device ID, ROM ID/title, filename, hash, token or save bytes. Retain the report itself as evidence. If a run stops partway through an existing-head qualification, keep the workspace: `original-remote-head.bin` holds the verified original head for manual restoration.

## Enabling a configuration

A configuration becomes `target_validated` only by a reviewed source change after its retained report passes. That change records the report here and in [MILESTONES.md](../MILESTONES.md). It does not start automatic synchronization by itself: the M4 daemon, production activity probes and approval gates remain required.

## M3 exit gate

At least three systems across two emulator families must pass both directions. The proposed minimum is DS and GBA (RetroArch) plus Dolphin GameCube GCI (Dolphin standalone). The gate is not met until those reports are retained.

## Deferred

- Multi-GCI and companion-file transfer: blocked until RomM can represent a multi-file group safely.
- Emulator-load verification of a committed download in the live save path: M7 real-device round trips.
- Production activity probes and the daemon: M4.
- NES, SNES, N64, PS1, PSP, Wii, 3DS, PS2 and Switch adapters: each advances through its own fixture and target gate.
