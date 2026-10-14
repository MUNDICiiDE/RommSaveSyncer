# RomM AutoSync — Development Milestones

**Version:** 1.2 (approved 2026-10-14)

**Scope:** Multi-system framework + Bazzite background daemon + desktop tray/dashboard + Gaming Mode controller UI and notifications

**Related document:** [FEATURES.md](FEATURES.md)

## Delivery strategy

Build the **framework for multiple platforms from day one**. Enable automatic synchronization **per adapter only after passing its safety tests**. The release goal is three validated systems across at least two emulator families; the application must inventory all 12 currently present platforms, clearly marking those not yet safe to synchronize. Subsequent adapters extend the same design rather than requiring a rewrite.

**Rule for every milestone:** Keep user saves untouched by default until the safety gates are met. Use fixtures and disposable test directories before any live write. Do not install services, change startup settings, or modify real saves without explicit approval.

## Milestone overview

| ID | Milestone | Core deliverable | Exit gate |
|---|---|---|---|
| M0 | Read-only environment and compatibility audit | Verified RomM protocol, emulator mappings, runtime constraints, implementation choice | Sign-off on inventory, risks and rollout |
| M1A | Pure reconciliation planner and safety contracts | Deterministic planner, adapter/activity interfaces, safety policies and CI | Planner is side-effect-free and its decision matrix passes in CI |
| M1B | Guarded local executor and durable state | Revalidation, state database, dry-run, backups, atomic writes and recovery | Automated no-data-loss and stale-plan tests pass |
| M2 | RomM integration | Device authentication and interoperable guarded sync | Cross-device protocol test in disposable environment passes |
| M3 | Multi-system emulator adapters | DS, GBA, Dolphin GCI and first wave of tested adapters | At least three systems/two families validated |
| M4 | Bazzite background automation | systemd user service, game activity monitoring, hooks and offline retry | Boot/mode-switch/crash tests pass |
| M5 | Desktop UX | Dashboard, conflict manager, searchable history, KDE tray, desktop notifications | Usable and tested end to end |
| M6 | Gaming Mode UX | Controller dashboard, Decky integration research/prototype, notification delivery | Controller workflow works; notification fallback documented |
| M7 | Full integration and failure testing | Real-device DS round trips, other systems, conflicts and recovery | Release acceptance criteria pass |
| M8 | Packaging, documentation and handoff | Installer, update/rollback, recovery, user/maintainer guides | Clean install and recovery independently verified |

---

## M0 — Read-only environment and compatibility audit

**Objective:** Eliminate guesses before writing any user files.

**Status:** Complete and approved on 2026-10-14. This approval permits M1A planning/implementation when explicitly requested; it does not authorize live save transfers, adapter enablement, service installation or configuration changes.

### Work items

- [x] Read deployed RomM version and document supported device sync endpoints, authentication, negotiation and `autosave` slot semantics.
- [x] Confirm the deployed RomM-visible DS contract: ROM identity, `autosave` slot, remote `.sav` filename, hashes/revision and conflict behavior.
- [x] Inventory observed Bazzite/EmuDeck/RetroArch/ES-DE/Steam paths, symlinks and configuration across all 12 targets; explicitly classify unconfigured or unexercised save behavior as disabled rather than guessing.
- [x] Validate the current Dolphin **per-game `.gci` folder** configuration: standalone selection, game identity, active Slot A, inactive Slot B, current one-file completeness and disposable backup/atomic replacement. Multi-GCI behavior remains an adapter enablement fixture gate.
- [x] Identify selected emulator per platform, including system/per-game alternatives, and distinguish core-specific RetroArch paths. GameCube and Wii use top-level `Dolphin (Standalone)` alternatives; no target has a per-game alternative, and other unconfigured systems inherit their recorded defaults.
- [x] Classify DS/GBA/NES/SNES/N64/GC/Wii/3DS/PS1/PS2/PSP/Switch adapters by risk and readiness.
- [x] Test systemd user lingering/session-switch constraints non-destructively; assess Decky/Game Mode UI options.
- [x] Research Bazzite packaging feasibility for the daemon, KDE tray/dashboard and controller UI, including user-local/native, Flatpak and other viable approaches under the immutable/update model.
- [x] Choose language/tooling, transport for local IPC, GUI strategy, credential storage, packaging direction and update model from documented proof-of-concept evidence.
- [x] Record approved representative-save observations and disposable staging evidence, and define fixture/rollback policy without committing save binaries, ROMs, BIOS data or secrets. Synthetic/emulator-generated fixtures remain required at the relevant adapter gates.
- [x] Produce the inventory report and approve the DS → GBA → Dolphin GameCube GCI early-validation order, with all unexercised adapters disabled.

**Deliverables:** `docs/environment-audit.md`, `docs/api-compatibility.md`, `docs/adapter-matrix.md`, `docs/safety-plan.md`, and a packaging/tooling decision record.

**Exit gate:** Passed on 2026-10-14. Real paths/protocol assumptions needed for planning are verified, unavailable behavior is explicitly disabled/labeled, packaging feasibility and implementation choices are documented, the DS → GBA → Dolphin GameCube GCI validation order is approved, and no implementation or live-save mutation has begun.

## M1A — Pure reconciliation planner and safety contracts

**Objective:** Define and prove synchronization decisions without performing I/O or mutating state.

**Status:** Implementation complete locally on 2026-10-14; awaiting review, commit/push and the first GitHub Actions run before the exit gate is approved.

### Work items

- [x] Define adapter contract: path resolution, game/slot identity, file groups, compatibility, safety predicates.
- [x] Define activity-detection interfaces that report game/emulator state conservatively without coupling the planner to Bazzite process inspection or launcher hooks.
- [x] Define explicit safety policies for unsupported adapters, ambiguous identity, active/unknown activity, missing mounts, incomplete file groups, incompatible formats, remote deletion and insufficient evidence.
- [x] Define normalized, immutable planner inputs for discovery snapshots, file/group hashes, RomM revisions and device baselines, adapter capabilities, activity observations and policy configuration.
- [x] Implement a deterministic, side-effect-free reconciliation planner that produces an ordered plan with stable reason codes for upload, download, no-op, conflict, defer, unsupported and error outcomes.
- [x] Prohibit planner access to the filesystem, database, process table, network, implicit current time, randomness or mutable global state; all such observations enter as explicit inputs.
- [x] Model stale-observation preconditions on every data-changing proposed action so the executor can revalidate them later.
- [x] Establish continuous integration and run planner unit, table-driven and property-oriented tests on every change.
- [x] Cover first-sync ambiguity, including a server timestamp-based direction with different content and no common baseline, plus simultaneous changes, remote deletion, identical content, incomplete groups, active/unknown games, unsupported adapters and deterministic ordering.

**Deliverables:** Core domain library, adapter and activity contracts, safety-policy model, deterministic planner, decision/reason schema, CI configuration and automated planner tests.

**Exit gate:** CI proves identical inputs produce identical ordered plans, the planner performs no I/O or mutation, every proposed mutation carries explicit revalidation preconditions, first-sync content divergence without a common baseline is a conflict regardless of timestamp ordering, unsafe and ambiguous scenarios never produce an executable write action, and real saves remain untouched.

**Exit-gate evidence:** Local `cargo fmt --all -- --check`, `cargo test --workspace --all-targets`, and `cargo clippy --workspace --all-targets -- -D warnings` pass on Windows. The suite contains 16 tests, including the required decision matrix, discovery-order permutations and a 6,480-case safety/precondition cross-product. No discovery or executor component exists, so no real save path is opened. Final gate approval remains pending a successful GitHub Actions run on both Windows and Linux.

## M1B — Guarded local executor and durable state

**Objective:** Execute approved plans safely while detecting stale observations and recovering from interruption.

### Work items

- [ ] Implement read-only discovery and command-line dry-run/status output using the M1A planner; export every decision with stable reason codes.
- [ ] Implement SQLite schema for device identity, game mapping, file/group hash, remote metadata, last common revision, retry queue, history, plans and conflicts.
- [ ] Add database migrations, transactional journaling and idempotent restart/recovery.
- [ ] Implement SHA-256 local integrity checks, stable-file detection, guarded atomic downloads and rollback.
- [ ] Before every save mutation, revalidate local file/group hashes, remote revision or device baseline, path and mount identity, disk space, adapter safety predicates and current emulator/game activity.
- [ ] Reject stale plans without modifying saves; record the changed precondition and trigger replanning or a visible conflict/blocked state.
- [ ] Implement backup rotation (default 10 routine versions/game), protected conflict archives and disk-space checks.
- [ ] Add per-game locks, prevention of duplicate transfer work, and explicit unsupported states.
- [ ] Add unit, integration, property-oriented and fault-injection tests for truncation, failed rename, changed hashes, changed remote revisions, mount replacement, activity races, ambiguous mapping, simultaneous changes and interrupted recovery.
- [ ] Extend CI to run executor, migration, restart/recovery and disposable-filesystem tests.

**Deliverables:** Discovery and dry-run CLI, guarded executor, state database and migrations, backup/rollback engine, audit journal and automated failure tests.

**Exit gate:** Disposable fixtures show no destructive action on invalid paths, stale plans, active/unknown games, ambiguous mappings, interrupted writes, missing/replaced mounts or conflicts; restart recovery is idempotent; all automated tests pass in CI; real saves remain untouched.

## M2 — RomM protocol and sync integration

**Objective:** Reliable two-way communication while preserving RomM's device/save-slot semantics.

### Work items

- [ ] Implement versioned API adapter for the actual deployed RomM protocol.
- [ ] Store dedicated device credentials securely with least privilege.
- [ ] Register/identify the Beelink as a distinct sync device where supported.
- [ ] Support protocol-requested hashes, slot metadata, server revisions, origin devices and integrity validation.
- [ ] Implement upload/download with temporary staging, explicit success confirmation and idempotent retries.
- [ ] Implement conflict detection based on last common version, not timestamps alone.
- [ ] Support offline queue, reconnect behavior, exponential backoff and request timeouts.
- [ ] Test deployed RomM `autosave` interoperability in both directions using safe disposable test saves.

**Deliverables:** RomM API adapter, integration tests, sanitized trace logs, error/error-recovery guide.

**Exit gate:** Both directions work in a disposable test environment, repeated requests do not duplicate saves, and conflicts never overwrite either side.

## M3 — Multi-system emulator adapters

**Objective:** Validate a broad framework against different real save models.

### Proposed staged adapter rollout

1. **Nintendo DS:** RetroArch melonDS DS live `.srm` ↔ byte-identical staged RomM `autosave` `.sav`; the existing two-way extension-rename test is the starting point.
2. **Game Boy Advance:** Per-game battery saves using the actual configured RetroArch core or standalone emulator.
3. **Dolphin GameCube GCI:** Verify title/region/slot ownership and all per-game `.gci` files as a coherent save set.
4. **Other simple RetroArch systems:** NES and SNES, followed by N64 only after its selected core and save model are verified.
5. **DuckStation PS1:** Only safe per-game memory-card configurations.
6. **Other platforms:** PSP, Wii, 3DS, PS2 and Switch — enable one by one as adapter tests prove compatibility.

### Work items

- [ ] Create automated fixtures with normal, missing, malformed, conflicting and multi-file saves for each adapter class.
- [ ] Test region/revision mismatches and multi-disc naming.
- [ ] Keep shared memory-card and incompatible save-state configurations disabled by default.
- [ ] Add platform readiness status and reasons for blocked configurations.
- [ ] Record matrix of platforms/emulators/configuration versions verified in actual use.
- [ ] Keep the adapter API and inventory capable of representing all 12 target platforms while enabling each concrete platform/emulator configuration only after its own validation gate passes.

**Deliverables:** Emulator adapter modules, validation fixtures, compatibility matrix, documentation.

**Exit gate:** At least **three systems and two emulator families** pass both directions; DS/GBA plus a standalone emulator are the proposed minimum, with Dolphin GCI prioritized as the standalone candidate. Do not claim 12-platform full compatibility unless all 12 have actually passed.

## M4 — Bazzite daemon and launch lifecycle

**Objective:** Synchronize in the background without depending on an open GUI.

### Work items

- [ ] Install a single-instance `systemd --user` service with restart-on-failure, restricted runtime permissions and detailed status commands.
- [ ] Validate/start on boot via user lingering if the user's configuration permits it; document security and behavior.
- [ ] Confirm the daemon survives **Gaming Mode ↔ Desktop Mode** transitions, reconnecting UI clients without restarting the sync engine.
- [ ] Discover supported running games/emulators and block writes while active.
- [ ] Build launch hooks where safe to guarantee pre-launch checks; document polling fallback where hooks cannot guarantee timing.
- [ ] Implement post-exit file-stability checks and online sync target (within 60 seconds when healthy).
- [ ] Restore queued work after service restarts, system reboot, network outages and NAS unmount/remount.
- [ ] Prevent two daemon instances and simultaneous competing writers.
- [ ] Produce disposable packaging prototypes for the selected Bazzite approach, covering daemon/user-service placement, desktop/tray integration, controller-dashboard launch, application data locations, update/rollback assumptions and uninstall cleanup.
- [ ] Add packaging smoke checks to CI where they can run without modifying a real Bazzite installation.

**Deliverables:** User unit/service files, launch-hook integration, reliable lifecycle/health tests, and disposable Bazzite packaging prototypes with installation/update/uninstall notes.

**Exit gate:** Cold boot, repeated Desktop ↔ Gaming Mode switching, forced crash/restart, and network reconnect do not lose tracked changes or overwrite saves in use; the selected packaging prototype installs and removes cleanly in a disposable representative environment without unsupported immutable-OS changes.

## M5 — Desktop dashboard and tray

**Objective:** Make sync health visible and conflicts recoverable without terminal commands.

### Work items

- [ ] Implement a desktop dashboard showing online/offline state, last confirmed sync, pending tasks, errors and per-platform adapter readiness.
- [ ] Show per-game last upload/download, device origin, timestamp, verified result and files affected.
- [ ] Add searchable persistent history with clear labels for checked/synced/skipped/failed.
- [ ] Implement conflict list and explicit actions: keep local, keep remote, retain both/defer.
- [ ] Include pre-resolution backup confirmation and backup restore/review workflow.
- [ ] Add KDE system tray icon with pause/resume, sync now, view activity and open dashboard.
- [ ] Add configurable native desktop success/failure/conflict notifications.
- [ ] Validate clean GUI reconnection after the desktop session is restarted.

**Deliverables:** Desktop dashboard, tray application, notification settings, conflict UI, accessibility/UX test notes.

**Exit gate:** User can confirm a completed sync, diagnose a failure, and resolve/restore a conflict without reading raw files or logs.

## M6 — Gaming Mode dashboard, Decky and notifications

**Objective:** Provide controller-first synchronization visibility in Bazzite Gaming Mode.

### Work items

- [ ] Deliver a fullscreen/controller-friendly dashboard usable as a non-Steam app.
- [ ] Prototype Decky Quick Access Menu plugin for daemon health, progress, history and unresolved conflicts.
- [ ] Verify Steam/Decky notification support on actual gamescope session and handle unavailable Decky gracefully.
- [ ] Ensure conflicts/failures remain accessible after notifications dismiss; avoid blocking active gameplay with modal dialogs.
- [ ] Implement controller-only conflict review, confirm and defer actions.
- [ ] Test connecting/disconnecting the Gaming Mode UI repeatedly while daemon remains alive.

**Deliverables:** Controller dashboard, optional verified Decky integration, tested notification route or documented fallback.

**Exit gate:** User can check sync health and resolve a safe conflict from Gaming Mode using a controller. Notification behavior is documented according to actual validation, not assumed.

## M7 — End-to-end integration, safety and resilience validation

**Objective:** Prove the full experience across the actual Beelink and RomM deployment.

### Required scenarios

- [ ] RomM `autosave` `.sav` → staged, verified and atomically committed EmuDeck RetroArch `.srm`; load progress.
- [ ] EmuDeck RetroArch `.srm` → byte-identical staged RomM `autosave` `.sav`; verify the server record and a subsequent download round trip.
- [ ] Validate at least three systems/two emulator families; include Dolphin `.gci` multi-file mapping once verified.
- [ ] Two devices modify the same game offline, reconnect, and neither change is lost.
- [ ] Service starts at boot, survives Desktop ↔ Gaming Mode switches, and recovers after crashing.
- [ ] RomM unavailable during gameplay; queued upload resumes safely when reconnected.
- [ ] Emulator continues writing when a remote update arrives; the local save is not touched.
- [ ] Interrupted download, bad checksum, full disk, missing mount and partial files do not corrupt originals.
- [ ] Backup can restore an earlier working save after user-selected resolution.
- [ ] Dashboard history accurately reports successful uploads, pulls, skips, conflicts and errors.
- [ ] Desktop alerts and Gaming Mode controller workflow pass practical testing.
- [ ] Post-exit sync target is measured; delayed cases have visible reasons.

**Deliverables:** Test reports, compatibility matrix, bug fixes, release-candidate validation log.

**Exit gate:** All release-blocking acceptance criteria in `FEATURES.md` pass, and remaining limitations are explicit rather than silently enabled.

## M8 — Release, operations and maintenance

**Objective:** Make installation, ongoing use, upgrades and recovery predictable.

### Work items

- [ ] Produce reproducible installer and configuration templates (no embedded secrets).
- [ ] Supply pause/resume/start/stop/status, dry-run, export logs, restore backup, update and uninstall commands.
- [ ] Provide database migration and rollback procedures, preserving previous state until success.
- [ ] Verify clean install, upgrade and uninstall on a representative Bazzite environment.
- [ ] Document RomM upgrades/API compatibility checks and how an adapter is validated before enabling.
- [ ] Publish user guide, troubleshooting, security model and developer/adapter guide.
- [ ] Mark supported/experimental/unsupported emulators and notification features in release notes.

**Deliverables:** Versioned install package, documentation, changelog, release notes and recovery guide.

**Exit gate:** A clean install and recovery can be repeated from documentation without manually reconstructing local configuration or risking save loss.

---

## Change control and cross-cutting quality gates

- **M0 approval:** Before scanning private system configuration beyond what the user consents to or making any modifications.
- **First live upload/download approval:** After reviewing a dry-run and known target filenames/slots.
- **Adapter approval:** Every newly enabled platform has a signed-off compatibility test.
- **System integration approval:** Before enabling systemd lingering, Steam hooks, or installing Decky components.
- **Release approval:** Only after the user verifies representative end-to-end saves on real hardware.

### Continuous engineering requirements

- Continuous integration begins in M1A with deterministic planner tests and expands in each milestone with executor failure tests, representative save fixtures, protocol integration, lifecycle and packaging checks.
- No credentials, tokens, BIOS files, copyrighted ROMs, or personally identifying paths in commits.
- Code review and documented migrations for anything touching user data.
- Every data-changing operation creates a structured audit event.
- Safety defaults cannot be bypassed by accidental configuration changes.
- Clearly separate **planned**, **implemented**, and **validated on target hardware** statuses.

## Recommended first Codex work item

> Build **M0 only**, as a non-destructive compatibility audit. Start with the committed `FEATURES.md` and `MILESTONES.md`. Inspect the deployed RomM API and actual Bazzite/EmuDeck configurations with the user's consent. Produce the audit reports and an adapter test matrix. Do not install daemons, activate automatic synchronization, modify existing saves, or collect secret tokens. Stop for approval before proceeding to M1A.

**Definition of project complete:** The daemon, tested adapter set, desktop and controller UI, conflict/recovery workflows, and installation package pass the acceptance criteria in `FEATURES.md`. Systems awaiting validation remain visibly unsupported rather than implicitly enabled.
