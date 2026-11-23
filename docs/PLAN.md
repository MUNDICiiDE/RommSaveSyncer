# RommSaveSyncer — Technical Plan

Status: **approved 2026-11-22** (Q1 answered: Kotlin, bundled-JRE folder on Bazzite is acceptable). Settled design: [GLOSSARY.md](../GLOSSARY.md), [ADR 0001](adr/0001-hybrid-launch-detection.md), [ADR 0002](adr/0002-gpl-3-license.md), [ADR 0003](adr/0003-autosave-slot-convention.md).

## Milestones

- [x] M1: Scaffolding and CI
- [x] M2: Dockerized RomM test harness
- [x] M3: RomM client and device authorization
- [ ] S1: Spike: Bazzite Game Mode (gamescope prompt, Secret Service, Steam shortcut)
- [ ] S2: Spike: Thor unwrapped-launch game identification and exit detection
- [ ] M4: Local state DB, secret store, outbox
- [ ] M5: Sync engine decision table and conflicts
- [ ] M6: sigil save units and cross-emulator conversion
- [ ] M7: Game Match
- [ ] M8: Desktop emulator registry (Windows first, Linux/EmuDeck paths)
- [ ] M9: Desktop service, IPC, Background Pull, offline queue
- [ ] M10: Wrapper CLI and wrapped launch flow
- [ ] M11: Desktop GUI, tray, controller navigation, fullscreen prompt
- [ ] M12: ES-DE hook installer (Windows)
- [ ] M13: Windows packaging, start-at-login, update check
- [ ] M14: Desktop process watcher (unwrapped launches)
- [ ] M15: Bazzite install, ES-DE/SRM/Steam hooks, Game Mode
- [ ] M16: Android shell: onboarding, foreground service, engine, Background Pull
- [ ] M17: Android unwrapped detection and post-exit upload
- [ ] M18: Thor bottom-screen dashboard and DS/3DS notifications
- [ ] M19: v1 hardening and release

Later (not v1): PS2, PSP, 3DS, Wii, Switch; RomM 5.5 snapshots/channels; optional NeoStation custom-launch contribution (issue #501).

## 1. Summary

- **Stack:** Kotlin everywhere. **Kotlin Multiplatform** (Kotlin 2.4.21) with **Compose Multiplatform 1.12.2**.
  - One shared core: the sync engine, RomM client, database and sigil adapter.
  - One shared Compose UI, running on the JVM desktop (Windows and Linux) and on Android.
  - Native Kotlin per platform where the platform requires it:
    - Android: foreground service, UsageStats, Shizuku and the bottom-screen Presentation.
    - Desktop: tray, process watcher, secret store and launcher hooks.
- **Why:** Android is the hardest target and needs Kotlin anyway. Argosy is Kotlin and already contains:
  - a working negotiate client;
  - sigil glue code;
  - Shizuku helpers;
  - Presentation code for the second screen.

  sigil ships a Kotlin/JNI binding that also builds for a desktop JDK.
- **Architecture:** each desktop device runs one long-lived **service process**, the only writer to SQLite. The **wrapper** (`RommSaveSyncer wrap …`) and the GUI talk to it over an AF_UNIX socket, which the JDK supports on Windows and Linux. The wrapper draws the fullscreen conflict prompt itself, so it is visible in gamescope. On Android, the service, GUI and Presentation share one process.
- **Sync decisions are made by the client** from per-game baselines (last synced local hash and server hash). The server's negotiate result is treated as advisory, because RomM 5.4.1's rules differ from ours (R1).
- **Tests:**
  - A fake RomM for fast engine tests.
  - Testcontainers with pinned `rommapp/romm:5.4.1` + MariaDB and two simulated Devices for integration tests.
  - GitHub Actions builds the Windows MSI, a Linux tarball and the APK.
- **Deviation from a settled requirement:** the Bazzite deliverable is a self-contained user-level folder (the app plus a bundled JRE, roughly 100 MB, estimated) under `~/.local/share`, not a single binary file. See open question Q1.

## 2. Requirements and constraints

**Hard (settled):** everything in GLOSSARY.md and ADRs 0001–0003. The design is in the grilling summary: wrapped launch on Windows and Bazzite; unwrapped launch and Background Pull elsewhere; the user always resolves conflicts; deletions are never synced; the `autosave` slot; device-approval sign-in; play sessions; v1 systems GB/GBC, GBA, NES, SNES, N64, DS, PS1 and GameCube.

**Provisional requirements, challenged by the research:**
- "Single user-level binary" on Bazzite: see Q1.
- "Socket.IO push" for Background Pull is not possible. In 5.4.1 the `/devices` namespace only emits `install:queued` and `install:cancelled`, and the docs say "the server never tells a device to start a sync". Background Pull therefore polls.
- Android permissions: showing the bottom-screen dashboard from the background also needs **Display over other apps** (SYSTEM_ALERT_WINDOW). Argosy declares it too. See Q5.

## 3. Language and stack

| Criterion | (a) Rust core + UniFFI + Kotlin Android + Tauri/Slint/egui | **(b) KMP + Compose MP** | (c) Flutter + native plugins | (d) .NET/Avalonia + separate Android |
|---|---|---|---|---|
| Android FGS / UsageStats / Shizuku / Presentation | Kotlin anyway, plus an FFI boundary | Native, same language | Kotlin via platform channels; Presentation needs a second engine | Kotlin anyway |
| UI codebases | 2 | **1** | 1 | 2 |
| Argosy reuse | Port to Rust | **Direct** | Port to Dart | Port |
| sigil | Own bindgen FFI + JNI on Android | **Existing Kotlin/JNI binding on both** | dart:ffi, own binding | C# fork exists |
| Controller UI on desktop | gilrs + own focus code | SDL3 bridge + own focus code | Plugin + own focus code | Own code |
| Tray (Windows + KDE) | tray-icon / ksni | ComposeNativeTray (native SNI on Linux) | tray_manager | Avalonia tray |
| Linux distribution | **True single binary**, low RAM | App folder + jlinked JRE, ~150–250 MB RSS (est.) | App folder + .so | App folder |
| Solo dev + AI | Two languages | **One language, one build** | Dart + Kotlin | C# + Kotlin |

**Pick: (b).** About 60% of the product's risk is Android-native, and Argosy already solves much of it in Kotlin. **What would change the answer:** if Q1 is "must be one small binary with low RAM", switch to (a). That means Slint 1.19 (GPL) on desktop, Compose on Android with UniFFI 0.32, and two UIs.

**Libraries (versions checked 2026-11-22):**
- Networking: Ktor 3.6.1 client (OkHttp 5.5 engine) + kotlinx.serialization.
- Database: SQLDelight 2.4.1 (sqlite-jdbc 3.54 on desktop).
- Tray: ComposeNativeTray 2.1.7.
- Gamepads: SDL 3.4 via JNA.
- Desktop OS access:
  - JNA for Win32 (Credential Manager, process command lines, Run key).
  - dbus-java 5.2 for the Secret Service.
- Android: Shizuku API 13.x.
- sigil: git submodule pinned at `v1.0.0`.
- Dependency injection: plain constructor injection.
- Build: Gradle 9.9 with a version catalog. The AGP version is picked in M1.

## 4. Architecture

**Modules**
- `core:model`: domain types (Game, SaveRef, SyncStatus, Conflict, PlaySession).
- `core:romm`: RomM API client.
  - Covers device auth, devices, negotiate, saves upload/download/confirm, sessions/complete, play-sessions, and roms (`/api/roms`, `/api/roms/by-hash`).
  - Errors are typed: a 409 guard; a 401 means re-authenticate.
  - Checks the server version via heartbeat and refuses anything older than 5.4.1.
- `core:db`: SQLDelight schema and migrations.
- `core:saves`: sigil adapter.
  - `resolve`/`collect`/`restore`, with sigil's state kept as an opaque blob.
  - An emulator-registry interface, and a compatibility matrix that refuses unsupported pairs.
  - GB/GBC/GBA/N64/DS/PS1/GC are fully "synced" by sigil. SNES/NES are only "located": raw files are passed through unchanged.
- `core:matching`: ROM identification.
  - A ROM hash cache keyed by (path, size, mtime).
  - A cache of the RomM catalog.
  - Filename suggestions and Manual Match.
- `core:engine`: the sync logic.
  - Per-game state machine, decision table and outbox.
  - Local Backups (10 kept).
  - Background Pull scheduler and play-session recorder.
  - Event bus (`SharedFlow`).
- `core:ipc` (JVM): versioned, length-prefixed JSON over `UnixDomainSocketAddress`, behind a `SyncerApi` interface with in-process and IPC implementations.
- `ui:shared`: Compose screens plus a controller focus layer. Gamepad input becomes D-pad/A/B key events: from SDL3 on desktop, natively on Android.
- `desktop:platform`: desktop OS integration.
  - Process watcher: Win32 command lines via JNA; `/proc` on Linux.
  - Secrets and autostart.
  - Desktop emulator registry: RetroArch cfg, EmuDeck `~/Emulation/saves`, Dolphin, DuckStation, melonDS, mGBA, Mupen/RMG.
- `desktop:hooks`: ES-DE custom_systems, SRM parser config, Steam `shortcuts.vdf`. Each has backup, undo, status and repair.
- `desktop:app`: one executable with subcommands `service | gui | wrap | hooks | status`.
- `android:app`: the Android app.
  - specialUse foreground service.
  - Onboarding.
  - UsageStats poller, Shizuku executor and FileObserver.
  - Presentation (bottom screen) and notifications.
  - Port of Argosy's `SavePathRegistry`.

**Local state, per game (`rom_id`, slot `autosave`)**
- `sync_state` holds:
  - `baseline_local_hash`: sigil `identity_hash`, which ignores clock ticks;
  - `baseline_server_hash`, plus the server save id and updated_at;
  - last sync time and device;
  - the sigil state blob;
  - status: InSync / RemoteNewer / UploadPending / Conflict / Offline / Error.
- `outbox` holds pending work:
  - kind: upload, archive_upload, play_session or complete_session;
  - a staged copy of the save, frozen at exit;
  - hash, attempt count and `next_attempt_at`.
- `conflicts` holds:
  - phase: prelaunch / postexit / background / first_sync;
  - for each side: time, device, emulator, size, whether it came from the current session, and the RomM screenshot;
  - the resolution.
- Plus: `local_backups`, `play_sessions`, `games`/`matches`/`exclusions`, `save_bindings` (emulator + Save Location + override) and `hooks`.

**Decision table** (L = the local save differs from the local baseline; R = the server head differs from the server baseline):
- No baseline, and both sides exist and differ → **Conflict**.
- Only R changed → download. This is the only case Background Pull acts on.
- Only L changed → upload with `overwrite=false`. A 409 response means re-evaluate as a Conflict.
- Both changed → Conflict.
- Server `delete` operations are ignored.
- The losing copy goes to RomM as a null-slot (archival) upload, or to a Local Backup.
- To avoid opening a sync session on every check, Background Pull polls the lighter `GET /api/saves?slot=autosave` and only negotiates when something changed.

**Wrapper flow** (`wrap --system gba --emulator retroarch:mgba_libretro -- <original cmd>`):
1. Connect to the service. If it isn't running, start it (systemd `--user start` on Linux, spawn on Windows) and wait about 3 s.
2. Send `PrelaunchSync`. The reply is Proceed, Conflict(details) or Offline.
3. On a conflict, show a fullscreen, always-on-top Compose prompt (Use RomM / Use local / Cancel), then send `Resolve`.
4. Launch the emulator and wait for it to exit. Send `SessionEnded` and pass through the emulator's exit code.
5. If the service is unreachable or RomM is offline, launch anyway and write a spool record that the service reconciles later. A launch is never blocked.

**Conflict flow:** the engine stores a conflict row and emits an event.
- Desktop: a tray badge and a notification.
- Android: the bottom-screen dashboard, or a notification for DS/3DS.
- Every GUI renders the same `ConflictScreen`.

## 5. Project structure

```
RommSaveSyncer/
├── build-logic/                 Gradle convention plugins (kotlin, android, compose, detekt)
├── gradle/libs.versions.toml    single version catalog
├── core/{model,romm,db,saves,matching,engine,ipc}/
├── ui/shared/                   Compose screens + controller focus system
├── desktop/{app,platform,hooks}/
├── android/app/
├── third_party/argosy-sigil/    submodule @ v1.0.0 (MPL-2.0, unmodified)
├── testing/romm-harness/        compose file, fixture ROM library, Testcontainers helpers
├── testing/fixtures/            synthetic saves; real-saves/ (gitignored, manifest committed)
├── docs/{adr,PLAN.md,manual-tests/}
└── .github/workflows/{ci.yml,release.yml}
```

## 6. Tooling

- **Already on the dev PC:** Temurin JDK 21, Docker 29.9, Rust 1.98, gh, PowerShell 7.6.8.
- **Still to install:** the Android SDK, NDK, CMake and adb. Gradle comes from the wrapper.
- **Docker Desktop** must be running for integration tests.
- **Lint:** ktlint, detekt and Android Lint.
- **Commands:**
  - `./gradlew check`: unit tests and lint.
  - `./gradlew integrationTest -Promm=docker`: the Docker RomM suite.
  - `./gradlew :desktop:app:run --args="service"`: run the service.
  - `./gradlew :desktop:app:packageReleaseMsi`: Windows installer (Windows only).
  - `./gradlew :desktop:app:createReleaseDistributable`: Linux tarball.
  - `./gradlew :android:app:assembleDebug`: debug APK.
- **sigil native builds:** CMake with `-DSIGIL_BUILD_JNI_HOST=ON` on desktop (MSVC on Windows, gcc on Linux), and sigil's own `bindings/android` CMake for the NDK.
- **CI:**
  - ubuntu-latest: unit tests, Docker integration tests, the Linux tarball and the APK.
  - windows-latest: unit tests, the sigil MSVC JNI build and the MSI.
  - Tagged releases upload all three artifacts.
- **Logging:** rolling log files, plus an "Export diagnostics" button.

## 7. Practices

- The engine is pure and suspend-based. Time, filesystem, RomM and sigil are injected, so every decision path can be unit-tested.
- No save is ever written without a Local Backup first. Writes go to a temp file and are then atomically renamed. Uploads use staged copies.
- Hook edits are idempotent, backed up, undoable and checked with golden-file tests. Repair compares hashes.
- A **dry-run mode** logs intended writes without touching anything. It is used the first time the app runs on a real device.
- Debug builds use a separate Android applicationId and data directory.
- Logs and bug reports record the RomM server version.

## 8. Milestone details

**Low-friction manual testing:** until M12/M15/M18, run the Docker RomM on the dev PC exposed on the LAN, and point devices at it. Install CI artifacts per PR, including `adb install -r` over Wi-Fi for the Thor. Use dry-run mode first. Each milestone has a checklist in `docs/manual-tests/`.

### M1: Scaffolding and CI
- Scope:
  - build-logic and the version catalog;
  - module skeletons;
  - sigil submodule with the host JNI library built by Gradle;
  - ktlint/detekt;
  - ci.yml.
- Acceptance:
  - `./gradlew check` passes on the dev PC;
  - a test calls sigil's version through JNI on the desktop JVM;
  - CI publishes a Windows MSI, a Linux tarball and a hello-world APK.
- Manual: install the MSI and see the hello window.
- Depends on: none.

### M2: Dockerized RomM test harness
- Scope:
  - compose file with `rommapp/romm:5.4.1` and pinned `mariadb`;
  - dummy ROMs for the 8 v1 systems;
  - a bootstrap that creates the admin user and runs a scan;
  - a JUnit extension.
- Acceptance:
  - heartbeat reports 5.4.1;
  - fixture ROMs get rom_ids and hashes;
  - a second run starts clean.
- Depends on: M1.

### M3: RomM client and device authorization
- Scope: `core:romm` and the device flow.
  - Device flow: `/api/auth/device/init`, then poll `/token`, handling `authorization_pending`, `slow_down`, `expired_token` and `access_denied`.
  - The test harness approves codes through the admin API.
- Acceptance:
  - auth → negotiate → upload (MD5) → download → confirm → complete with play_sessions works end-to-end against Docker;
  - a stale upload's 409 guard is surfaced as a typed error.
- Tests: all endpoints; 401 and token expiry; multipart fields; null-slot upload.
- Depends on: M2.

### S1: Spike: Bazzite Game Mode
- Scope: a throwaway Compose window started from a non-Steam shortcut wrapping a dummy script.
- Acceptance: written findings with screenshots on:
  1. whether gamescope shows the wrapper's fullscreen window, and gives focus back to the emulator afterwards;
  2. whether SDL3 sees the controller there;
  3. whether the Secret Service is unlocked in Game Mode after autologin.
- Manual: the user runs it on the Bazzite box.
- Depends on: M1.

### S2: Spike: Thor game identification
- Scope: a throwaway APK that logs:
  - UsageStats events;
  - Shizuku `ps` liveness;
  - candidate "current game" sources: RetroArch `content_history.lpl`, emulator config or recent-files lists, FileObserver hits;
  - where NeoStation stores its scraped media (box art, fanart), and whether it is readable from shared storage.
- Acceptance: for each Thor emulator, written findings on how the game and its exit are detected, plus the Thor's Android version.
- Manual: the user runs it on the Thor.
- Depends on: M1.

### M4: Local state DB, secret store, outbox
- Scope:
  - SQLDelight schema with a migration test;
  - `SecretStore`: Windows Credential Manager; the Secret Service (plus the fallback chosen in Q4); an Android Keystore-wrapped token;
  - a durable outbox with backoff.
- Acceptance:
  - state survives a process kill in the middle of a transaction;
  - the token round-trips on Windows;
  - the outbox replays exactly once after a restart.
- Depends on: M1.

### M5: Sync engine
- Scope:
  - the decision table;
  - conflict records;
  - Local Backups (capped at 10);
  - null-slot upload of the losing copy;
  - server `delete` ignored;
  - play-session queue.
- Acceptance: every table row is covered against the fake server, and against Docker with two Devices:
  - cross-device sync;
  - first-sync conflict;
  - post-exit conflict held locally;
  - offline queue drained on reconnect;
  - nothing ever deleted on RomM or locally.
- Tests: table-driven and property tests (random device actions never lose a version).
- Depends on: M3, M4.

### M6: sigil save units
- Scope:
  - collect/restore for GB/GBC/GBA/N64/DS/PS1/GC;
  - raw pass-through for SNES/NES;
  - the compatibility matrix.
- Acceptance:
  - fixtures round-trip with the same content_hash;
  - RetroArch mGBA `.srm` ↔ mGBA `.sav` both ways;
  - DeSmuME `.dsv` ↔ melonDS `.sav`;
  - an unsupported pair is refused with a message.
- Depends on: M5.

### M7: Game Match
- Scope:
  - ROM scan with hash cache;
  - RomM catalog fetch (`updated_after`);
  - hash → rom_id;
  - filename suggestions and Manual Match;
  - per-system and per-game excludes.
- Acceptance:
  - the fixture library matches 100% by hash;
  - a renamed ROM still matches;
  - an unmatched ROM gets suggestions;
  - a Manual Match persists.
- Depends on: M3, M4.

### M8: Desktop emulator registry
- Scope:
  - detection and Save Locations for RetroArch (honouring `savefile_directory` and the sort-by-core/content settings), Dolphin, DuckStation, melonDS, mGBA and Mupen/RMG;
  - EmuDeck layouts;
  - asking when several emulators exist for a system;
  - per-game override.
- Acceptance: fixture install trees resolve to the right paths on Windows and Linux; overrides win.
- Manual: a "detected emulators" printout on both PCs.
- Depends on: M6.

### M9: Desktop service
- Scope:
  - `service` subcommand with a single instance;
  - AF_UNIX IPC;
  - Background Pull (polling; skipped while an emulator runs or the local save has changed);
  - `status` and `sync-now`.
- Acceptance:
  - a pull happens within one interval after another Device uploads;
  - it is skipped while a watched emulator runs;
  - IPC clients get live events.
- Manual: run on Windows against the Docker RomM over the LAN.
- Depends on: M5–M8.

### M10: Wrapper
- Scope:
  - `wrap` subcommand;
  - spool fallback;
  - exit-code passthrough;
  - play-session recording.
- Acceptance:
  - with a fake emulator, pre-launch download → launch → post-exit upload → play session works;
  - with the service down or RomM offline, launch is delayed ≤2 s and the upload happens later;
  - added latency when already in sync is <1.5 s (measured).
- Tests: every branch, Cancel, and an emulator crash.
- Depends on: M9.

### M11: Desktop GUI
- Scope:
  - dashboard;
  - game detail with Save Versions + restore and Local Backups;
  - conflict screen with both sides' details;
  - settings and hook status;
  - tray status icon;
  - close prompt "minimize to tray or quit" with remember;
  - notifications for conflicts and errors;
  - SDL3 controller navigation;
  - the wrapper's fullscreen prompt.
- Acceptance: every screen works with a controller only; the conflict prompt blocks the launch until a choice is made.
- Depends on: M10.

### M12: ES-DE hook (Windows)
- Scope:
  - edit `~/ES-DE/custom_systems` (es_systems / es_find_rules) to add a labelled wrapped command per system, optionally selecting it;
  - backup, undo, repair and GUI status.
- Acceptance: golden-file tests for apply, re-apply (no change), undo (byte-identical restore) and repair after an overwrite.
- Manual: an ES-DE launch syncs end-to-end, first against the Docker RomM, then against the real server.
- Depends on: M11.

### M13: Windows packaging
- Scope:
  - per-user MSI;
  - start-at-login (HKCU Run);
  - update check against GitHub releases, with a notification.
- Acceptance: clean install, upgrade and uninstall on Windows 11, with no leftover processes.
- Depends on: M12.

### M14: Desktop process watcher
- Scope:
  - detect an emulator starting, and its ROM from the command line;
  - "newer on RomM" warning;
  - upload after exit.
- Acceptance: RetroArch started outside ES-DE warns when RomM is newer and uploads after exit.
- Depends on: M9.

### M15: Bazzite
- Scope:
  - tarball plus `RommSaveSyncer install`: `~/.local/share`, a systemd user unit and a desktop entry;
  - ES-DE Linux hook;
  - SRM parser and `shortcuts.vdf` wrapping around EmuDeck's launcher scripts, which are never edited;
  - Steam shortcut for the GUI (with permission, while Steam is closed);
  - repair.
- Acceptance: launches from desktop ES-DE and from Game Mode both sync; the conflict prompt is visible in gamescope.
- Manual: the Game Mode checklist.
- Depends on: M12, M14, S1.

### M16: Android shell
- Scope:
  - onboarding: All files access, Usage access, Shizuku, notifications, Display over other apps, battery optimization;
  - specialUse foreground service started on boot;
  - engine + sigil Android binding;
  - `SavePathRegistry` port;
  - Background Pull.
- Acceptance:
  - the service runs after a reboot;
  - a save uploaded from the PC appears on the Thor automatically when the local save is unchanged.
- Depends on: M9, S2.

### M17: Android unwrapped detection
- Scope:
  - UsageStats polling and Shizuku liveness;
  - game identification per S2;
  - upload after exit;
  - degraded mode without Shizuku.
- Acceptance: for each v1 emulator on the Thor, start and exit are detected and the save uploads after exit; nothing syncs during play.
- Depends on: M16.

### M18: Thor bottom screen
- Scope:
  - Presentation at the bottom screen's native 1240×1080 while a single-screen game runs: title, box art and the game's background art (fanart) behind a dark scrim, status, last sync, session timer. Artwork comes from RomM's own media for the matched game first; NeoStation's scraped media is an optional local source if S2 finds it in shared storage;
  - "newer on RomM — quit, then Download" prompt;
  - DS/3DS notifications instead of the dashboard;
  - hand the screen back to NeoStation after play. Outside play RommSaveSyncer has no visible window on either screen: it keeps running in the background (foreground service with a silent, minimised notification, which Android requires). The full GUI opens only when launched from the app drawer or the notification.
- Acceptance: the dashboard appears and hides with each play session; Download waits for the emulator to close.
- Manual: the Thor checklist.
- Depends on: M17.

### M19: v1 hardening and release
- Scope: diagnostics export, release workflow, signing and user docs.
- Acceptance: a tagged build installs on all three devices, and a 7-day soak loses no saves.
- Depends on: all.

## 9. Risks

- **R1. RomM's rules differ from ours.** RomM 5.4.1 `comparison.py` decides a first sync with no history by **timestamp**, not by conflict, and the server can send `delete` operations. Mitigation: the client makes its own decisions from the decision table, tests pin 5.4.1 behaviour, and the app refuses older server versions. Found in M3:
  - RomM pairs saves on only the newest row per slot, and every slotted upload creates a new row. So a device's sync history doesn't carry over to another device's newer upload, and "both changed" comes back as `upload`, not `conflict`.
  - The upload response's `updated_at` has microseconds, but the stored value (TIMESTAMP on MariaDB) is whole seconds. Sync baselines (M4+) must take the server timestamp from a re-read or from negotiate, never from the upload response.
- **R2. The Thor can't easily tell which game started.** Android reports only the emulator's package, not the ROM. Mitigation: spike S2, a "Which game?" prompt as fallback, and NeoStation #501 later.
- **R3. Shizuku must be restarted after each reboot** unless the device is rooted. Without it there's no `Android/data` access and no reliable exit detection. RetroArch's HOME button doesn't flush SRAM (sigil `saves-on-android.md`).
- **R4. Game Mode visibility.** The gamescope prompt might not show, and the Secret Service might be locked in Game Mode. Mitigation: spike S1.
- **R5. EmuDeck and Steam overwrite our hooks.** An EmuDeck reset or update rewrites `custom_systems/es_systems.xml`, `es_find_rules.xml` and the SRM configs. Steam rewrites `shortcuts.vdf` on exit. Mitigation: hash-based repair detection, and editing only while Steam is closed.
- **R6. JVM size and speed** (estimated ~100 MB on disk, 150–250 MB RAM, 0.5–1 s wrapper start). Mitigation: SerialGC, a small heap, AppCDS; measured in M10.
- **R7. sigil is young** (v1.0.0), and SNES/NES are only "located". Mitigation: pin the tag and contribute upstream.
- **R8. RomM keeps only 10 autosave versions by default.** Mitigation: losing copies go to the null slot.
- **R9. Hashing large disc images** (GameCube, CHD) is slow. Mitigation: the hash cache, and the CHD's internal sha1.
- **R10. Windows SmartScreen** warns about unsigned installers.

## 10. Resolved questions (2026-11-22)

- **Q1.** Bazzite install as a self-contained folder with a bundled JRE: **accepted**. The stack stays Kotlin.
- **Q2.** The Thor runs **Android 13**. Root status is unknown, so assume not rooted: Shizuku is required for `Android/data` access. Target SDK stays current, so Android 14/15 foreground-service rules still apply in code.
- **Q3.** Windows ES-DE is **set up by EmuDeck for Windows**. Most systems use individual standalone emulators, not RetroArch for everything. M8 detects them; there is no fixed per-system list.
- **Q4.** If the Secret Service is locked, the token **may fall back to a 0600 file**.
- **Q5.** "Display over other apps" and the battery-optimization exemption: **allowed**.
- **Q6.** The losing copy goes to RomM as an **extra null-slot version**, in addition to a Local Backup. Background Pull interval: **5 minutes**.
- **Q7.** **No paid Windows certificate.** Unsigned installers downloaded from GitHub show a one-time SmartScreen warning ("More info → Run anyway"). Android APKs are signed with a **free self-generated release key**, kept in GitHub Actions secrets with a user-held backup. Losing that key would block in-place updates.

## 11. Sources (accessed 2026-11-22)

- RomM device sync protocol: https://docs.romm.app/latest/developers/device-sync-protocol/
- RomM 5.4.1 source (`handler/sync/comparison.py`, `endpoints/saves.py`, `endpoints/device/auth.py`, `endpoints/sockets/`): https://github.com/rommapp/romm/tree/5.4.1
- RomM compose example: https://github.com/rommapp/romm/blob/master/examples/docker-compose.example.yml
- argosy-sigil: https://github.com/rommapp/argosy-sigil
- Argosy (`NegotiatorSaveSyncStrategy.kt`, `SavePathRegistry.kt`): https://github.com/rommapp/argosy-launcher
- NeoStation: https://github.com/misobadev/neostation-frontend
- EmuDeck ES-DE script: https://github.com/dragoonDorise/EmuDeck/blob/main/functions/ToolScripts/emuDeckESDE.sh
- ES-DE INSTALL.md: https://gitlab.com/es-de/emulationstation-de/-/blob/master/INSTALL.md
- Android 15 behavior changes: https://developer.android.com/about/versions/15/behavior-changes-15
- Shizuku setup: https://shizuku.rikka.app/guide/setup/
- Compose native distributions: https://kotlinlang.org/docs/multiplatform/compose-native-distribution.html
- ComposeNativeTray: https://github.com/kdroidFilter/ComposeNativeTray
