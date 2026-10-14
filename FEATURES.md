# RomM AutoSync — Feature Specification

**Version:** 1.2 (approved 2026-10-14)

**Target environment:** Beelink SER8 running Bazzite and EmuDeck with a Synology-hosted RomM server

**Project approach:** Multi-system architecture for all 12 target platforms from day one; enable each emulator/platform adapter only after its compatibility and safety tests pass

**Related document:** [MILESTONES.md](MILESTONES.md)

## 1. Purpose

Create an independent background service that automatically synchronizes game saves between local Bazzite emulators and **RomM as the central hub**. Games may be launched from EmulationStation Desktop Edition (ES-DE), Steam shortcuts, or another supported launcher. ROM library management and other clients are outside this project's normal operating boundary.

The service must protect save data, operate across Bazzite Desktop Mode and Gaming Mode, work during network outages, and offer a usable desktop and controller-friendly Gaming Mode interface with status, history, notifications, and conflict resolution.

## 2. Guiding principles

1. **Never silently lose progress.** Back up before replacement, use atomic writes, and preserve both sides of a conflict.
2. **Do not disturb active gameplay.** Never overwrite a save while its game/emulator may be writing it.
3. **Gameplay does not depend on networking.** An unavailable RomM server must not prevent launching a game.
4. **Do not assume interchangeability.** Save-file extension, format, region, game identity, emulator version, and save-state formats must be considered separately.
5. **Multi-system framework, tested adapters.** The core must support multiple emulator families immediately, but automatic writes stay disabled for unverified adapters.
6. **Single background engine, multiple frontends.** A GUI crash, missing Decky installation, or session change must not stop syncing.
7. **Inspectable and reversible.** Every synchronization action has a recorded reason and a recovery path.
8. **Plan first, execute separately.** Reconciliation decisions come from a deterministic, side-effect-free planner that can be tested without filesystem, database, process, or network access.
9. **Never trust a stale plan.** Immediately before changing a save, the guarded executor must revalidate expected file hashes, remote revisions, path identity and emulator/game activity.
10. **Continuously verified.** Automated tests and continuous integration begin with the planner milestone and remain required for every subsequent milestone.

## 3. Current platform inventory

The following systems were visible in the user's RomM library on 2026-10-13. Counts are a **snapshot, not a hard-coded limit**. Save locations and emulator assignments must be discovered during M0.

| Platform | Games shown | Initial candidate adapter / save model | Validation notes |
|---|---:|---|---|
| Game Boy Advance | 18 | RetroArch and/or mGBA, per-game battery saves | Verify actual emulator and extensions |
| Nintendo 3DS | 17 | Azahar, per-title directories | Verify title IDs, encryption/profile details |
| Nintendo 64 | 4 | RetroArch or standalone per-game saves | Verify selected core and save model |
| Nintendo DS | 17 | RetroArch melonDS DS live `.srm` ↔ RomM `autosave` `.sav` through byte-identical staging | **Two-way extension-rename compatibility tested successfully by user** |
| Nintendo Entertainment System | 1 | RetroArch per-game saves | Verify core and file locations |
| Nintendo GameCube | 18 | Dolphin per-game `.gci` files | User reports GCI-folder storage, **not a shared card image**; verify game IDs, slots, and folder structure |
| Nintendo Switch | 4 | Per-title/per-user save directories | Per-emulator and firmware/profile mapping; high-risk, gate separately |
| PlayStation | 1 | DuckStation memory cards | Verify per-game cards vs shared cards and multi-disc behavior |
| PlayStation 2 | 17 | PCSX2 cards or per-game save data | Shared memory cards **must remain disabled** unless safely isolated |
| PlayStation Portable | 14 | PPSSPP `SAVEDATA` directories | Resolve game IDs and complete directory ownership |
| Super Nintendo Entertainment System | 3 | RetroArch per-game saves | Verify core and file locations |
| Wii | 3 | Dolphin Wii title-specific save files/directories | Separate from GameCube `.gci` adapter |

**Total shown:** 117 games across 12 platforms. Do not assume every game has an existing save or that every platform is immediately safe to sync.

### Adapter categories

- **Single-file per-game:** DS, GBA, NES, SNES, some N64 implementations.
- **Multiple files per game:** Dolphin GameCube `.gci` saves; must associate each file with the correct game and slot.
- **Game-specific memory cards:** DuckStation and other supported per-game arrangements.
- **Per-title directory trees:** PPSSPP, 3DS, Wii, some Switch emulators.
- **Shared memory cards or shared directories:** Not automatically synchronized until safely attributable to one game.
- **Save states:** **Out of scope for automatic transfer in v1**, unless a separate, explicitly tested adapter is approved. Never assume cross-core compatibility.

## 4. Functional requirements

### F-01 — Emulator and ROM discovery

- Discover EmuDeck's configured ROM, save, and BIOS *paths* without writing to BIOS files.
- Detect RetroArch core/save directory configuration, including sort-by-core/system settings and symlinks.
- Discover standalone emulators and their actual active save profiles.
- Resolve RomM ROM IDs using reliable metadata and mappings; do not guess based solely on similar titles.
- Handle revisions, regions, filename differences, multi-disc collections, and user-configured game paths.
- Show unsupported/ambiguous games in the dashboard with a reason and no automatic save writes.

### F-02 — Pluggable emulator adapters

Each adapter declares:

- Supported platforms, emulator/core IDs, and minimum verified conditions.
- Save locations and path ownership: single file, multiple files, or entire directory.
- Game identity rules: title/region/serial/product ID and profile/slot if relevant.
- Save-format compatibility and any permitted transformations.
- Safe read/write behavior, completeness checks, and conditions that block transfers.
- How to determine whether its game is currently running.
- Status: **validated**, **pending validation**, **unsupported**, or **temporarily unsafe**.

**Nintendo DS special case:** The user's tested workflow proves that a specific `.sav` and RetroArch melonDS DS `.srm` can load the same bytes after extension renaming. The Beelink client synchronizes only with RomM; other devices and clients are outside the normal adapter contract.

For validated compatible DS configurations, the adapter must:

- Upload by copying the inactive local `.srm` into temporary staging with the RomM-side `.sav` name, verifying that the staged bytes match, then uploading it to the mapped RomM ROM and `autosave` slot.
- Download the RomM `.sav` into temporary staging, verify identity, size and expected hashes, revalidate the remote baseline plus the local `.srm` hash and emulator inactivity, back up the live `.srm`, then atomically replace it while retaining the `.srm` filename.
- Never rename, move or expose the live `.srm` as `.sav`, and never decide that one side is newer from filesystem timestamps alone.
- Preserve both versions whenever RomM negotiation indicates an independent remote change or cannot prove a single safe direction.

**Dolphin GameCube special case:** Support the reported per-game `.gci` folder workflow, not an assumed shared `.raw` memory card. Ensure all files in a game/slot group are mapped and backed up as a consistent unit. Dolphin Wii saves use a separate adapter.

The adapter API must be capable of representing all 12 target platforms and their save models even while most adapters remain disabled. Adapter status is granted individually from validation evidence; support for one RetroArch core, Dolphin mode, or directory layout does not imply support for related configurations.

### F-03 — RomM API integration

- Authenticate using an independent, least-privilege credential and register/identify this Beelink as a distinct device where supported.
- Request only the catalog and sync scopes required for safe identity mapping and transfer: `roms.read`, `platforms.read`, `assets.read`, `assets.write`, `devices.read`, and `devices.write`. Do not request ROM/platform write or administrative scopes.
- Verify the deployed RomM version's actual sync endpoints, request/response schema, upload/download semantics, revision handling, and slot rules **during M0**.
- Interoperate with the deployed RomM **`autosave` slot convention**, subject to live verification; avoid creating parallel save histories through incorrectly assigned slots.
- The audited deployment currently uses the default `autosave` slot for every console and has no user-configured slot/channel overrides. Treat that as deployment evidence, not a universal constant: validate the slot returned by RomM for every planned operation and block unknown or mismatched slots.
- Preserve and interpret origin device, server revision/update metadata, and content hashes where available.
- Support the hash algorithm expected by the deployed RomM negotiation API; use SHA-256 internally for local integrity if useful. Do not presume that local SHA-256 is the wire protocol.
- Make the RomM protocol implementation replaceable to accommodate changes between RomM releases.
- Never log credentials, full tokens, or HTTP Authorization headers.

### F-04 — Two-way synchronization and scheduling

- Sync on service start and when connectivity returns.
- Detect newly imported ROMs and safely initialize eligible saves.
- Fetch eligible newer remote saves while the game is idle.
- Upload changed local saves after gameplay finishes and files stabilize.
- Poll remote changes approximately every **5 minutes** while idle (configurable).
- Target **within 60 seconds of detected game exit** for online, successful post-game sync; report if the target cannot be met.
- Prevent duplicate transfers with hashes, tracked revisions, an idempotent local journal, and per-game synchronization locks.
- Never infer a clean common ancestor from timestamps alone.

### F-05 — Launch and active-game safety

- Track game/emulator activity for Steam shortcuts, ES-DE, and supported launch methods.
- Integrate optional supported **pre-launch hooks** to fetch newer saves *before* the emulator opens them.
- Understand that observing process start does **not** guarantee pre-launch syncing: if a hook is unavailable, defer unsafe writes and make the limitation clear.
- Avoid local replacement while the related emulator/game is running; use conservative blocking if game identity cannot be determined.
- After exit, wait until file writes have settled, then rescan and upload.
- The daemon must **never block launching a game solely because RomM is offline**.

### F-06 — Reconciliation planning, versioning, conflicts, and backups

- Maintain a local SQLite ledger of known saves, server identity/revision, last common synchronized content, and pending operations.
- Feed discovery results, ledger snapshots, remote metadata, adapter capabilities and observed activity into a deterministic, side-effect-free reconciliation planner.
- Given the same normalized inputs, the planner must produce the same ordered plan and reason codes. It must not read or write files, query processes, access the network, use the current clock implicitly, generate random identifiers, or mutate the database.
- Represent proposed uploads, downloads, skips, conflicts, unsupported states and safety blocks as explicit plan records that can be inspected and tested independently of execution.
- Classify results as: **in sync**, **local newer**, **remote newer**, **local only**, **remote only**, **conflict**, **pending/offline**, **unsupported**, or **error**.
- Treat independent local and remote changes from the last common known version as a **conflict**; preserve both without automatically selecting a winner.
- On first connection, run read-only discovery and **dry-run reconciliation** before any transfers; do not assume older timestamps identify dispensable saves.
- When local and remote content differ and no verified common baseline exists, classify the first-sync state as a **conflict** even if RomM's negotiation fallback proposes a direction from timestamps. A server-designated slot head identifies the current remote version, not the safe winner.
- Execute an approved plan only through a guarded executor. Immediately before each mutation, it must revalidate local file/group hashes, relevant server revision or sync baseline, path and mount identity, available space, and emulator/game activity. Any mismatch invalidates the planned action and triggers replanning or a visible blocked/conflict state.
- Before replacing any local save, make a recoverable backup; validate downloaded content and use same-filesystem atomic replacement where practical.
- Default to **10 routine backup versions per game**, configurable with a storage cap; never silently prune **unresolved conflict copies**.
- On disk-space shortage, interrupted download, invalid save, mismatch, or missing volume, refuse the risky write and surface an actionable status.
- Do not automatically propagate destructive deletions in v1.
- Allow explicit, confirmed resolution: **keep local**, **keep remote**, or **keep both / defer**. Preserve recovery copies for every resolution.

### F-07 — Offline operation and recovery

- Keep changes local and durable while RomM is unreachable; retry with exponential backoff and jitter.
- Persist queued work across crash, reboot, and network changes.
- Avoid duplicate uploads, stale overwrites, and conflicts after reconnecting multiple devices.
- Preserve original saves if an atomic write or network transfer fails.
- On service restart, reconcile the journal safely before resuming unfinished work.

### F-08 — Bazzite startup and mode switching

- Run the sync engine as a **systemd user service** with restart-on-failure behavior.
- Support starting automatically on cold boot using user lingering **only after assessing and enabling the necessary local configuration**.
- Survive transitions **Desktop Mode ↔ Gaming Mode** without duplicate daemons or lost work.
- Reattach whichever UI is available in the active graphical session; the daemon has no dependency on an open graphical window.
- Keep executable/config/data under locations compatible with Bazzite's immutable/update model; avoid unsupported system modifications.
- Report missing/mounted storage or unavailable network instead of writing to unintended fallback folders.
- Prototype packaging and installation against Bazzite's immutable/update model before the service milestone exits, including user-service placement, application data locations, desktop integration and a recoverable uninstall path.

### F-09 — Desktop user interface and tray

- Provide a compact status dashboard and KDE-compatible system tray integration.
- Display overall state, online/offline status, last successful sync, outstanding work, conflict count, and adapter support by system.
- Offer per-game status with timestamps, transfer direction, file list, save source device, and reason for any skip/error.
- Maintain persistent, searchable synchronization history across sessions.
- Provide safe manual controls: **Sync now**, **Pause**, **Resume**, **Dry run**, **View backup**, and **Resolve conflict**.
- Show configurable desktop notifications on success, failure, and conflict; avoid notification spam by default.

### F-10 — Gaming Mode interface and notifications

- Provide a controller-navigable dashboard accessible from Steam Gaming Mode as a non-Steam app.
- Prototype a **Decky Loader** Quick Access Menu integration for live status/history and conflict alerts.
- Validate any Decky/Steam notification integration on the user's actual Bazzite setup; **do not assume Plasma notifications appear inside gamescope**.
- If Decky or native Steam notifications are unavailable, the standalone controller dashboard remains the supported fallback.
- Display persistent conflict/failure indicators; do not interrupt an active game with modal dialogs.
- Permit conflict resolution with a controller, including preview of both versions' metadata and explicit confirmation.

### F-11 — Logging and observability

- Record timestamps, result, game/RomM ID, platform, emulator adapter, direction, file count, transferred bytes, version/hash reference, and relevant diagnostics for each transfer.
- Distinguish **checked** from **actually synced**; a green check must represent verified sync, not a failed/no-op check.
- Retain history in a local database, configurable retention for verbose operational logs, and human-readable exports without secrets.
- Provide command-line status/diagnostics for recovery when neither UI is available.
- Make audit and dry-run reports exportable for troubleshooting.

### F-12 — Installation, security, and maintainability

- Package reproducible install, update, rollback, backup restore, and uninstall instructions/scripts.
- Store credentials with user-restricted permissions; avoid exposing authentication in GUI logs or shell history.
- Authenticate local GUI/daemon communication, or use user-scoped IPC with appropriate permissions.
- Document adapter creation, schema migrations, tests, configuration, and troubleshooting.
- No destructive migration or deployment without explicit user approval.
- Research packaging feasibility during M0, including native/user-local, Flatpak and other viable Bazzite-compatible approaches; record constraints and a recommended direction before implementation is committed to a packaging model.
- Produce disposable packaging prototypes during the daemon milestone so service installation, Desktop/Gaming Mode compatibility and update assumptions are validated before release hardening.
- Establish continuous integration in M1A with deterministic planner tests. Add executor fault-injection, adapter fixtures, protocol integration and packaging checks as their milestones begin.

## 5. User-experience states

| UI state | Meaning | Typical action |
|---|---|---|
| Synced | Latest eligible save confirmed to match RomM | None |
| Syncing | Upload/download in progress | View progress |
| Pending | Local changes awaiting safe transfer | Wait / Sync now |
| Offline | RomM unavailable; local play unaffected | Retry later |
| Conflict | Local and remote diverged | Compare / keep local / keep remote / keep both |
| Paused | User temporarily disabled automatic transfers | Resume |
| Unsupported | Adapter/game cannot be safely mapped | Inspect reason |
| Error | Transfer or integrity check failed | Review logs / retry |

## 6. Explicit non-goals (v1)

- Downloading ROMs or replacing RomM's library-management functionality.
- Syncing BIOS, controller profiles, emulator settings, or RetroAchievements.
- Converting save *states* between different emulators/cores.
- Automatically merging two independently progressed game saves.
- Automatically synchronizing unpartitioned/shared memory cards.
- Guaranteeing Steam popup notifications before a Bazzite/Decky proof of concept.
- Guaranteeing compatibility for every platform before individual adapters pass validation.

## 7. Release acceptance criteria

- [ ] **AC-01 Install/boot:** Service starts automatically and survives reboots and Bazzite updates without modifying the immutable OS.
- [ ] **AC-02 Session switching:** Survives repeated Desktop ↔ Gaming Mode switches without duplicate services or loss of pending work.
- [ ] **AC-03 Discovery:** Correctly identifies local save locations, game IDs, profiles, and safe file groups; ambiguous IDs are blocked.
- [ ] **AC-04 Multi-system:** End-to-end synchronization is demonstrated for **at least three systems using two emulator families**; DS and GBA plus one separately configured emulator are the initial targets, with GCI GameCube prioritized for early testing.
- [ ] **AC-05 DS download:** RomM `autosave` `.sav` → staged, verified and atomically committed EmuDeck `.srm` loads correctly.
- [ ] **AC-06 DS upload:** EmuDeck `.srm` → byte-identical staged RomM `autosave` `.sav` succeeds without renaming the live `.srm`.
- [ ] **AC-07 Automatic:** No manual Push/Pull or file renaming is needed after the daemon is configured.
- [ ] **AC-08 Launcher coverage:** Steam shortcuts and ES-DE are validated; all other launch paths have documented safe behavior.
- [ ] **AC-09 Game activity safety:** No writes to active game saves; no game launch is blocked by offline RomM.
- [ ] **AC-10 Conflicts:** Independent changes from two devices remain recoverable and require explicit resolution.
- [ ] **AC-11 Backup/recovery:** Restore a save after a conflict, interrupted write, failure, and manual resolution.
- [ ] **AC-12 Offline:** Work queued while offline survives reboot and transfers safely after reconnection.
- [ ] **AC-13 GCI:** Correct `.gci` mapping and coherent grouped backups on GameCube; no unrelated save is touched.
- [ ] **AC-14 Unsupported:** Shared/unknown save layouts and incompatible states are skipped with intelligible diagnostics.
- [ ] **AC-15 History:** UI reliably distinguishes verified upload/download from checks, skips, and failures.
- [ ] **AC-16 Desktop UI:** KDE tray and popup notifications display health, errors, and conflicts; safe resolution works.
- [ ] **AC-17 Gaming UI:** Controller dashboard works; Decky integration and notification path are either tested successfully or have a documented fallback.
- [ ] **AC-18 Security:** Local credentials and history are protected; logs contain no secrets.
- [ ] **AC-19 Maintenance:** Install, pause, resume, update, rollback, backup restore, and uninstall procedures are tested.
- [ ] **AC-20 Post-game timing:** Under normal online conditions, detected game-exit changes are reported as synchronized within 60 seconds, or a reason for delay is visible.
- [ ] **AC-21 Deterministic planning:** Repeated planner runs over identical normalized inputs produce an identical ordered plan and reason codes without filesystem, database, process, clock, randomness or network side effects.
- [ ] **AC-22 Guarded execution:** A changed local hash, file group, remote revision, mount/path identity or emulator activity state causes the executor to reject the stale action without modifying the save.
- [ ] **AC-23 Continuous verification:** CI begins with M1A and exercises planner decisions, executor failure paths, adapter fixtures, protocol integration and packaging checks as those components are introduced.
- [ ] **AC-24 Packaging:** The chosen Bazzite packaging model supports installation, automatic user-service startup, Desktop/Gaming Mode transitions, update/rollback and uninstall without unsupported immutable-OS modifications.

## 8. Open decisions for M0

1. Verify exact installed RomM version, sync API endpoints, `autosave` slot metadata and deployed server behavior.
2. Inventory actual save paths and configured cores for all 12 platforms; especially GameCube GCI folder mapping and Wii save directory layout.
3. Confirm whether EmuDeck/ES-DE/Steam launch hooks can guarantee a pre-launch check without noticeable latency.
4. Test whether `systemd --user` plus linger behaves as intended through the user's Bazzite boot and session switching.
5. Verify Decky availability, version and Gaming Mode notification mechanism; choose fallback if unsupported.
6. Determine preferred implementation language, local IPC transport, GUI framework, credential-storage approach and update model from compatibility proof of concept.
7. Choose default routine backup disk cap and precise retention policy; protected conflicts are excluded from automatic cleanup.
8. Research Bazzite packaging feasibility, comparing viable user-local/native, Flatpak and other approaches for the daemon, desktop/tray UI and controller UI; document constraints without installing a production service.
9. Agree initial rollout order. Prioritize **Nintendo DS → Game Boy Advance → Dolphin GameCube GCI**, then consider NES/SNES, DuckStation PS1, N64/PPSSPP/Wii and finally 3DS/PS2/Switch according to safety evidence.

**Safety gate:** No automatic overwrite or deletion of existing user saves is permitted until M0 has identified the relevant files, a dry-run result is reviewed, and the user approves live synchronization.
