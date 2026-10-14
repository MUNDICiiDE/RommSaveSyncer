# M0 Environment Audit

**Specification:** 1.2

**Audit started:** 2026-10-14

**Status:** M0 audit complete and approved 2026-10-14; unconfigured adapters remain explicitly disabled pending individual validation

## Purpose and safety boundary

This report records the deployed RomM, Bazzite, EmuDeck, RetroArch, ES-DE, Steam, emulator and storage configuration needed to design RomM AutoSync without guessing.

M0 is non-destructive. During this milestone:

- Do not install or enable services, enable user lingering, add launch hooks, change startup settings, or install Decky components.
- Do not upload, download, rename, replace, delete or edit any live save.
- Do not read or record credentials, authorization headers, session cookies, BIOS files or ROM contents.
- Do not copy a real save into the repository. Fixtures must be synthetic or explicitly approved, copied, sanitized and documented.
- Keep public documentation, user-reported facts and observations from the deployed systems clearly separated.

The project workspace is on a Windows development host. No Bazzite or RomM filesystem is directly available in this workspace, so target observations use approved, user-provided sanitized output.

## Evidence states

| State | Meaning |
|---|---|
| Verified | Observed on the named target with a reproducible read-only check |
| User-reported | Supplied by the user but not independently observed during M0 |
| Documented | Supported by a cited upstream primary source, but not yet verified on the target |
| Inferred | Reasonable interpretation that still needs verification |
| Unknown | No evidence yet |
| Blocked | Cannot be checked without additional access, approval or a safe test environment |

## Target inventory

| Target | Known facts | M0 state | Evidence still required |
|---|---|---|---|
| Beelink SER8 | Bazzite Deck/Kinoite 44, stable image `44.20261012`; kernel `7.2.9-ogc2.1.fc44.x86_64`; rollback deployment `44.20261006.1`; EmuDeck installed | System facts and Desktop/Gaming/Desktop lifecycle shape verified; EmuDeck user-reported | EmuDeck version |
| RomM server | Synology Container Manager deployment of RomM 5.4.0 using `rommapp/romm:latest`, accessed without a reverse proxy; container auto-restart is enabled; live heartbeat includes all 12 target platform slugs; user reports games for all targets | Version/UI, image tag, auto-restart, heartbeat, platform coverage, six-scope pairing and authenticated read-only device/platform/save/ROM access verified | Image digest and local-ROM-to-RomM identity mapping |
| Development host | Windows workspace at `X:\Coding Projects\RommSaveSyncer` | Verified | No target-runtime claims should be derived from this host |

## Approved access method

On 2026-10-14, the user approved a guided audit using sanitized command output that they run and provide. The project will not initiate a connection to the Beelink or RomM server.

Collection will proceed in small read-only batches. Before sharing output, the user may replace usernames, hostnames, IP addresses, domain names, mount labels, device serials and unrelated application names with descriptive placeholders. Tokens, cookies, passwords, authorization headers, ROM filenames and save contents must be omitted entirely.

## Bazzite and session audit

The first user-provided read-only batch was collected on 2026-10-14 from a TTY session. Environment variables from that shell cannot establish the desktop or gamescope session environment.

| Item | Required evidence | State |
|---|---|---|
| OS identity | Bazzite 44, Kinoite, `bazzite-deck`, stable; booted image `44.20261012` | Verified |
| Deployment model | Signed ostree container image with previous deployment retained | Verified |
| Kernel | `7.2.9-ogc2.1.fc44.x86_64` | Verified |
| Desktop | Desktop Mode reports KDE on Wayland; Plasma shell is present and the user manager imports Wayland, display and desktop variables | Verified |
| Gaming Mode | Probe supplied after the mode switch observed one gamescope process, no Plasma shell, one Wayland session, active default/graphical-session targets, inactive Plasma and gamescope-session targets, and no imported Wayland/display variables in the user manager | Verified mode shape; user manager reachable but transiently degraded with nine failed units, all cleared after returning to Desktop |
| User manager | `systemctl --user is-system-running` returned `running`; login state returned `active` | Verified |
| Lingering | `Linger=no`; do not change it during M0 | Verified |
| Autostart | Desktop: default, graphical-session and Plasma targets active; gamescope-session target inactive. Gaming: default and graphical-session targets active, while Plasma and gamescope-session targets are inactive despite a live gamescope process. On return to Desktop, Plasma and graphical targets were active again and the user manager was healthy | Verified round-trip observation; gamescope-session target is not a usable lifecycle signal on this deployment |
| EmuDeck storage | `<user>/Emulation` is on the internal NVMe-backed Btrfs `/var/home` mount, writable | Verified |
| Save indirection | EmuDeck save roots use symlinks into Flatpak data directories and `Emulation/storage`/`Emulation/roms` trees | Verified |
| Target mounts | Individual save targets have not yet been checked with `findmnt`; do not yet assume every target shares the EmuDeck mount | Unknown |
| Sandboxing | Installed Flatpak IDs, declared permissions and user overrides for RetroArch, Dolphin, melonDS and PPSSPP | Verified; each user override grants `host` filesystem and network access |
| Notifications | Plasma notification behavior in Desktop Mode and available Gaming Mode/Decky route | Unknown |
| Controller UI | Non-Steam application launch and controller focus behavior | Unknown |

The Desktop Mode lifecycle baseline observed one Wayland session, no X11 session, one TTY session, a running user manager, one Plasma shell process, no gamescope process and eleven Steam/Steam-helper process matches. Three user sessions reported active; this count is retained as an observation rather than treated as proof of three independent graphical desktops. The user manager's environment contained `WAYLAND_DISPLAY`, `DISPLAY` and `XDG_CURRENT_DESKTOP`. Lingering remained disabled. This provides the comparison baseline for the Gaming Mode observation below.

The follow-up supplied after switching the local console to Gaming Mode accidentally retained the probe's manual `mode=desktop` label; the label does not drive any measurement. The observed state had one gamescope process, no Plasma shell, ten Steam/Steam-helper matches, one Wayland session, no X11 session and one TTY session. `default.target` and `graphical-session.target` remained active; `plasma-workspace.target` became inactive. `gamescope-session.target` remained loaded but inactive even though gamescope was running, so it cannot be used as the sole activity or lifecycle signal. The user manager no longer exposed `WAYLAND_DISPLAY` or `DISPLAY` but retained `XDG_CURRENT_DESKTOP`; lingering remained disabled. The manager remained reachable but reported `degraded` with nine failed user services: eight categorized as Plasma/KDE and one as a Flatpak application. None were gamescope, Steam, portal, audio, D-Bus, mount, socket or target units.

After returning to Desktop Mode without resetting failed units, gamescope was absent, Plasma was present, the user manager returned to `running`, and all nine failures cleared. The graphical-session and Plasma workspace targets were active, the gamescope-session target remained inactive, and `WAYLAND_DISPLAY`, `DISPLAY` and `XDG_CURRENT_DESKTOP` were again present in the manager environment. The round trip therefore recovers the existing desktop session state automatically. It also reinforces two design constraints: the background daemon must attach to a non-graphical user lifecycle, while the tray and Gaming Mode UI must be session-specific clients that receive display connectivity from their launch context. This observation does not prove that a future daemon survives the transition; that requires the disposable M4 service prototype.

## Observed Flatpak permissions

The four installed emulator Flatpaks were inspected without changing their configuration. All declare shared IPC and network access, `devices=all`, and broad host filesystem access. No system-bus policy entries were reported. Each application's three-line user override contains only a `[Context]` section that sets `filesystems=host;` and `shared=network;`; no override environment or bus-policy entry was reported.

| Application | Display/audio sockets | Filesystem observations | Session-bus policy count | Packaging consequence |
|---|---|---|---:|---|
| RetroArch (`org.libretro.RetroArch`) | Wayland, fallback X11, PulseAudio | `host`, PipeWire runtime, read-only gamescope socket and auxiliary Discord/KDE paths | 6 | Current emulator access is broad enough for its EmuDeck layout, but is not a least-privilege template for AutoSync |
| Dolphin (`org.DolphinEmu.dolphin-emu`) | X11, PulseAudio | `host`, read-only gamescope socket and auxiliary Discord/KDE paths | 6 | Installed Dolphin exposes X11 rather than Wayland; UI packaging must be tested independently under both Desktop and Gaming Mode |
| melonDS (`net.kuribo64.melonDS`) | Wayland, X11, fallback X11, PulseAudio | `host`, `home` and KDE configuration | 3 | Standalone melonDS is not the selected DS path, and its broad access grants no authority to widen the AutoSync GUI sandbox |
| PPSSPP (`org.ppsspp.PPSSPP`) | Wayland, X11, PulseAudio | `host` and read-only gamescope socket | 1 | Confirms another EmuDeck emulator uses broad host access; the PSP adapter remains individually unvalidated |

The preferred separation remains a normal user-scoped daemon with explicit configured save roots plus a narrower graphical client. The daemon can rely on ordinary user filesystem permissions and perform all authenticated file/network work; a future Flatpak GUI should receive only the IPC and portal permissions it actually needs. Emulator Flatpak grants must not be copied into RomM AutoSync merely for convenience.

## Native runtime and credential-service inventory

The Desktop Mode SSH environment has no `rustc`, Cargo, Node.js, pnpm or `flatpak-builder` executable on `PATH`. Podman, Distrobox and Homebrew are available. This makes the Beelink suitable for installation and physical qualification but not the assumed release build host; release artifacts should be built reproducibly in CI and must not require a compiler or JavaScript toolchain after installation.

The host linker cache reports WebKitGTK 4.1 and JavaScriptCoreGTK 4.1, but not WebKitGTK 4.0. It also reports `libappindicator3`, `libsecret-1` and SQLite 3, while `libayatana-appindicator3` was not found. These are feasibility signals only: the packaging prototype must still prove the exact ABI, tray behavior and runtime dependency set on the immutable target.

`secret-tool` is available and `org.freedesktop.secrets` was owned on the Desktop Mode user bus by KDE's `ksecretd` process at inspection time. The bus associated it with the overall `user@1000.service`, not a dedicated persistent unit. The credential-related units are a transient KWallet desktop-portal service and the static `plasma-kwallet-pam.service`. The Secret Service name did not appear in the bus's activatable-name list.

In settled Gaming Mode, four gamescope-related processes and fifteen Steam-related processes were present, Plasma was absent, the user manager was reachable but degraded, and `org.freedesktop.secrets` was not owned. The earlier zero-gamescope/zero-Plasma sample is classified only as an intermediate transition and is not used as Gaming Mode evidence. KWallet/Secret Service cannot be the sole store for automatic synchronization because a daemon starting or restarting in Gaming Mode could not retrieve its token. A user-restricted credential file is the reliable baseline unless target inspection proves that user-scoped `systemd-creds` encryption works across the same lifecycle; Secret Service may remain an optional Desktop integration. Tokens must never be exposed to the WebView, command line or logs.

The target provides `systemd-creds` 259. Its installed help exposes both `--user` encryption and explicit UID binding, and `systemd-creds has-tpm2` reports complete firmware, driver, system, subsystem and userspace-library support. Upstream systemd documents that user-scoped encrypted credentials are intended for per-user service managers and bind the user identity and machine identity into the encryption key.

The first disposable Gaming Mode proof explicitly requested the TPM-only key type together with `--user`. Encryption was refused before any transient unit ran with `Selected key not available in --uid= scoped mode, refusing.` The shell wrapper then exited the interactive SSH shell because it incorrectly used top-level `exit 1`; its `EXIT` trap removed the runtime directory. This was a test-script error, not a Bazzite logout or service failure. The result establishes that TPM-only encryption is not a valid UID-scoped choice. A subsequent read-only check found no existing `/var/lib/systemd/credential.secret`; M0 will not initialize system-wide credential state merely to test the scoped host or host+TPM alternatives.

The v1 credential backend is therefore an owner-only file at `$XDG_CONFIG_HOME/romm-autosync/credentials.json` (falling back to `~/.config/romm-autosync/credentials.json`), within a mode-`0700` directory and with mode `0600`. The daemon—not the WebView or shell—must receive the pairing response and create the file using symlink-safe, exclusive/atomic operations, verify owner/type/mode before every load, and refuse permissive or ambiguous paths. The file contains only the normalized server origin, device ID, scoped token, expiry and granted scopes; logs, exports and SQLite must never contain the token. This protects against other OS users but not arbitrary code already executing as the same user, a limitation that must be documented. Secret Service may be an optional Desktop-only integration, and encrypted systemd credentials may be revisited only through a separately approved packaging prototype.

Reference: [systemd `systemd-creds` documentation](https://github.com/systemd/systemd/blob/main/man/systemd-creds.xml)

## EmuDeck, ES-DE and Steam audit

Upstream ES-DE documentation says its ROM directory is configurable, alternative emulators can be selected globally or per game, custom event scripts are opt-in, and Linux paths are case-sensitive. The Beelink audit subsequently located the live ES-DE application data at `<user>/ES-DE`, read `settings/es_settings.xml` and `custom_systems/es_systems.xml`, and inspected the AppImage's bundled Unix definitions through a disposable extraction. A sanitized structural scan found that GameCube and Wii each persist the system-level `Dolphin (Standalone)` selection as a top-level `alternativeEmulator` block in a gamelist fragment, outside any game record. No per-game alternative was found. Discovery must therefore merge bundled definitions, top-level system alternatives and any future per-game metadata in precedence order.

Reference: [ES-DE user guide](https://gitlab.com/es-de/emulationstation-de/-/blob/master/USERGUIDE.md)

The deployed audit must record:

- EmuDeck version and configured `Emulation` root.
- Whether the root is internal, removable, network-backed or symlinked.
- ES-DE application home, ROM root, enabled custom systems and selected alternative emulator per platform/game.
- Steam shortcuts and launch commands for representative games.
- Whether ES-DE event scripts are enabled and whether they provide usable pre-launch and post-exit events.
- Every symlink in the ROM/save path chain, including resolved target, mount identity and behavior while unavailable.

## Emulator and save-path inventory

All 12 ES-DE definition-default labels and command families have now been observed on the Beelink. GameCube and Wii have verified top-level alternatives selecting standalone Dolphin; no per-game alternative was found. The DS and GameCube gamelists each contain one game; all other discovered target gamelists are empty, and GBA/NES/SNES/N64 have no gamelist file. Other unconfigured targets inherit their defaults. This does not prove that every referenced executable/core is installed, that every system has been launched, or that its candidate save root is active and safe.

| Platform | Candidate family | Actual emulator/core | Save root | Save model | State |
|---|---|---|---|---|---|
| Nintendo DS | RetroArch 1.22.2 and standalone melonDS 1.1 are installed Flatpaks | RetroArch `melonDS DS` 1.4.0, selected in ES-DE | `<user>/.var/app/org.libretro.RetroArch/config/retroarch/saves` | Per-game battery save; no same-stem companion observed for the candidate | M0 core, root, identity, remote head and extension-only staging verified |
| Game Boy Advance | RetroArch 1.22.2 is installed | RetroArch `mGBA` 0.10-dev, selected in ES-DE | `<user>/.var/app/org.libretro.RetroArch/config/retroarch/saves` | Per-game battery save plus possible RTC data | Core and global root verified; zero local GBA ROMs/saves, so adapter remains disabled pending a fixture |
| Nintendo GameCube | Dolphin 2609a is installed as a Flatpak and RetroArch is installed | Top-level ES-DE system alternative `Dolphin (Standalone)`; standalone settings apply to the ES-DE-launched game | `<user>/.var/app/org.DolphinEmu.dolphin-emu/data/dolphin-emu/GC` | Region- and slot-aware GCI folder | Selection and single-GCI contract verified; multi-GCI fixture remains open |
| Nintendo Entertainment System | RetroArch 1.22.2 is installed | ES-DE default RetroArch `mesen` (`Mesen`) | Shared RetroArch root is a candidate | Usually per-game battery save where applicable | System default verified; installation/core version, local content and save mapping unvalidated |
| Super Nintendo Entertainment System | RetroArch 1.22.2 is installed | ES-DE default RetroArch `snes9x` (`Snes9x - Current`) | Shared RetroArch root is a candidate | Per-game battery save plus possible RTC data | System default verified; installation/core version, local content and save mapping unvalidated |
| Nintendo 64 | RetroArch 1.22.2 is installed; standalone candidates unknown | ES-DE default RetroArch `mupen64plus_next` (`Mupen64Plus-Next`) | Shared RetroArch root is a candidate | Core-dependent save files | System default verified; installation/core version, local content and file grouping unvalidated |
| Wii | Dolphin 2609a is installed as a Flatpak and RetroArch is installed | Top-level ES-DE system alternative `Dolphin (Standalone)` | `<user>/.var/app/org.DolphinEmu.dolphin-emu/data/dolphin-emu/Wii` candidate | Per-title directory tree | Selection verified; local launch/save-model validation remains open |
| Nintendo 3DS | No matching Flatpak appeared in the initial filtered list | ES-DE custom default `Citra` invokes RetroArch; exact core token unresolved | Shared RetroArch root is a candidate; observed Azahar data is selection-mismatched | Core-specific save model | System default family verified; exact core/install and active root unvalidated |
| PlayStation | RetroArch 1.22.2 is installed; no DuckStation Flatpak appeared in the filtered list | ES-DE default RetroArch `mednafen_psx` (`Beetle PSX`) | Shared RetroArch root is a candidate | Per-game or shared memory card | System default verified; installation/core version, card mode and identity unvalidated |
| PlayStation 2 | No matching PCSX2-family Flatpak appeared in the initial filtered list | ES-DE default RetroArch `pcee2` (`PCEE2`) | Shared RetroArch root is a candidate | Folder card, per-game card or shared card | System default verified; installation/core version, card mode and root unvalidated |
| PlayStation Portable | PPSSPP 1.20.4 and RetroArch 1.22.2 are installed Flatpaks | ES-DE default RetroArch `ppsspp` | Shared RetroArch root is the applicable candidate; standalone `SAVEDATA` is not inferred active | Core-specific save model | System default verified; core version, active root and complete ownership unvalidated |
| Nintendo Switch | No matching emulator Flatpak appeared in the initial filtered list | ES-DE custom-system default `Ryujinx (Standalone)` | `<user>/.config/Ryujinx/bis/user/save` candidate | Per-title/per-user directory tree | System default verified; installation, active user/profile and complete group unvalidated |

## Observed EmuDeck save links

The following sanitized links were observed on 2026-10-14. A listed link establishes path configuration, not adapter safety or the emulator selected by ES-DE for every game.

| EmuDeck link | Sanitized target | Relevance |
|---|---|---|
| `<user>/Emulation/saves/retroarch/saves` | `<user>/.var/app/org.libretro.RetroArch/config/retroarch/saves` | Candidate DS/GBA/NES/SNES/N64/PS1 save root |
| `<user>/Emulation/saves/retroarch/states` | `<user>/.var/app/org.libretro.RetroArch/config/retroarch/states` | Save states; excluded from v1 automatic transfer |
| `<user>/Emulation/saves/dolphin/GC` | `<user>/.var/app/org.DolphinEmu.dolphin-emu/data/dolphin-emu/GC` | GameCube root requiring region/slot/GCI inspection |
| `<user>/Emulation/saves/dolphin/Wii` | `<user>/.var/app/org.DolphinEmu.dolphin-emu/data/dolphin-emu/Wii` | Wii root requiring per-title ownership inspection |
| `<user>/Emulation/saves/dolphin/StateSaves` | `<user>/.var/app/org.DolphinEmu.dolphin-emu/data/dolphin-emu/StateSaves` | Save states; excluded from v1 automatic transfer |
| `<user>/Emulation/saves/ppsspp/saves` | `<user>/.var/app/org.ppsspp.PPSSPP/config/ppsspp/PSP/SAVEDATA` | PSP per-title save root candidate |
| `<user>/Emulation/saves/ppsspp/states` | `<user>/.var/app/org.ppsspp.PPSSPP/config/ppsspp/PSP/PPSSPP_STATE` | Save states; excluded from v1 automatic transfer |
| `<user>/Emulation/saves/azahar/saves` | `<user>/Emulation/storage/azahar/sdmc` | 3DS data root requiring title/profile boundaries |
| `<user>/Emulation/saves/azahar/states` | `<user>/.local/share/azahar-emu/states` | Save states; excluded from v1 automatic transfer |
| `<user>/Emulation/saves/ryujinx/saves` | `<user>/.config/Ryujinx/bis/user/save` | Switch user-save root requiring profile/title mapping |
| `<user>/Emulation/saves/ryujinx/saveMeta` | `<user>/.config/Ryujinx/bis/user/saveMeta` | Switch metadata that may affect group completeness |
| `<user>/Emulation/saves/ryujinx/system_saves` | `<user>/.config/Ryujinx/bis/system/save` | System save area; excluded unless explicitly attributable and validated |
| `<user>/Emulation/saves/ryujinx/system` | `<user>/.config/Ryujinx/system` | Emulator system data; outside automatic save-sync scope |

Additional links for Cemu, RPCS3, shadPS4, Vita3K, xemu and Xenia were present but those platforms are outside the current 12-platform target set. Their existence is recorded only to prevent a future broad scanner from treating all of `Emulation/saves` as in scope.

The user reports retaining the default RomM `autosave` slot for every console and making no custom slot/channel selections. Live API responses must still confirm this before any adapter is enabled.

## Observed RetroArch configuration

The following effective global values were reported from the RetroArch Flatpak configuration on 2026-10-14:

| Setting | Sanitized value | Consequence |
|---|---|---|
| `autosave_interval` | `10` | RetroArch may flush SRAM every 10 seconds during play; activity and post-exit stability checks are mandatory |
| `savefile_directory` | `<user>/Emulation/saves/retroarch/saves` | Confirms the EmuDeck save link is the configured root |
| `savefiles_in_content_dir` | `false` | Saves are not intentionally colocated with ROM content |
| `sort_savefiles_by_content_enable` | `false` | No global content-directory sorting |
| `sort_savefiles_enable` | `false` | No global core sorting according to this setting |
| `savestate_directory` | `<user>/Emulation/saves/retroarch/states` | States are separate and remain out of v1 scope |
| `savestates_in_content_dir` | `false` | States are not intentionally colocated with ROM content |
| `sort_savestates_by_content_enable` | `false` | No global content-directory sorting for states |
| `sort_savestates_enable` | `false` | No global core sorting for states |

The save root contains a top-level directory named `melonDS DS`, but the user's relevant save is at the root. A read-only count found one `.srm` candidate and one `.sav`; the user confirmed the `.sav` is a manual renamed/copied test artifact for the same game and it must be ignored by discovery. Both are 8,192 bytes but are not currently byte-identical, consistent with one copy changing after the test; this is not evidence of format conversion. The artifact matches exactly one RomM version, the current `Latest` `autosave`, so its remote storage lineage is verified without exposing its hash. No same-stem companion file was observed. Approved read-only inspection established that the sole candidate `.srm` is 8,192 bytes, had modification time `2026-10-14 10:59:10.082342110 -0400`, MD5 `0829f71740aab1ab98b33eae21dee122` and SHA-256 `9f1dcbc35c350d6027f98be0f5c8b43b42ca52b7604459c0c42be3aa88913d47`. These values are a baseline fingerprint, not proof of cross-emulator compatibility.

The initial Flatpak runtime query returned no RetroArch core extension entries. Direct read-only inspection of RetroArch's configuration tree later found both `melonds_libretro.so` and `melondsds_libretro.so`; their presence explains why both core-specific configuration directories exist.

RetroArch's configuration tree includes override directories for many cores, including `melonDS`, `melonDS DS`, `mGBA`, `Mupen64Plus-Next`, `Nestopia`, `Snes9x`, `Beetle PSX`, `SwanStation`, `dolphin_emu` and `PPSSPP`. The `melonDS` directory contains `melonDS.opt` and `nds.cfg`; `melonDS DS` contains `melonDS DS.cfg` and `melonDS DS.opt`. No save-directory or save-sorting directive was found in those two override directories. Core metadata reports generic melonDS as an unspecified `Git` build and the ES-DE-selected `melonDS DS` core as version 1.4.0.

## Observed Dolphin layout

The configured Dolphin Flatpak data root contains GameCube region directories `EUR`, `JAP` and `USA`. An initial read-only count returned zero total files. A later scan observed one `.gci` under `USA` and one `.raw` file, demonstrating that save discovery can change during the audit and must never be treated as a durable execution precondition. The two candidate ROM directories resolve to one target and their two path observations identify one unique `.rvz`, so discovery must deduplicate aliases before planning.

The GCI has a present header, valid game and maker codes, a non-empty internal name and a two-block declaration. Its 16,448-byte length equals the 64-byte directory entry plus two 8,192-byte blocks, and its header region agrees with the `USA` path. Dolphin's read-only `dolphin-tool header --json` result returned a six-byte disc identity that exactly matches the GCI header's four-byte game code plus two-byte maker code. This proves local ownership by the sole deduplicated `.rvz`. Independent CRC, MD5 and SHA-1 lookups of that RVZ each returned the same single RomM ROM, and its platform is `ngc`, completing the local-save-to-RomM identity chain without relying on names. The observed `.raw` is only 68 bytes, is not referenced by the main or per-game memory-card configuration, and cannot be a GameCube raw memory-card image. It is excluded from the GCI save group; its exact auxiliary role does not need to be inferred for synchronization.

The Dolphin configuration was located at `<user>/.var/app/org.DolphinEmu.dolphin-emu/config/dolphin-emu/Dolphin.ini`, separate from the data root.

No explicit `SlotA`, `SlotB`, `GCIFolderAPath`, `GCIFolderBPath`, `MemcardAPath` or `MemcardBPath` entry was returned from `Dolphin.ini`. The returned `SIDevice0 = 6` and `SIDevice1 = 0` values describe controller ports and are not memory-card settings. The user subsequently confirmed that the installed Dolphin UI configures Slot A as `GCI Folder` and Slot B as empty/None. Seventy-four per-game settings files contain zero memory-card override keys, so no per-game exception to the UI configuration was found.

## Public path and layout hints

These entries guide inspection and must not be hard-coded:

- EmuDeck documents a configurable `Emulation` root and commonly exposes save paths beneath `Emulation/saves` using symlinks.
- EmuDeck documents GameCube and Wii save links beneath `Emulation/saves/dolphin`, targeting the Dolphin Flatpak data directory in a typical SteamOS installation.
- Dolphin source defines separate GCI folder paths for slots A and B and derives default region directories. Its GCI loader identifies current-game files from GCI header game codes, so filename matching alone is insufficient.
- ES-DE may select alternative emulators globally or per game; filesystem inspection alone is not sufficient without reading launcher configuration.

References:

- [EmuDeck file management](https://github.com/EmuDeck/emudeck.github.io/blob/main/docs/file-management/steamos/file-management.md)
- [Dolphin GCI folder path configuration](https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/Core/Config/MainSettings.cpp)
- [Dolphin GCI directory implementation](https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/Core/HW/GCMemcard/GCMemcardDirectory.cpp)

## Approved read-only collection plan

The approved access method is user-provided sanitized command output. Output must be reviewed locally for secrets and personally identifying paths before sanitized facts are committed.

1. Record OS, desktop/session and installed application versions.
2. List relevant Flatpak application IDs and inspect permissions without changing them.
3. Read emulator and launcher configuration files needed to resolve active paths and cores.
4. Resolve symlinks and mount identities without traversing unrelated directories.
5. Inventory names, sizes, timestamps and hashes only for explicitly approved save fixtures or narrowly approved representative saves.
6. Observe existing `systemd --user`, linger and session state without enabling or disabling anything.
7. Observe Desktop to Gaming Mode behavior without installing a service or hook.

## Open blockers

- The Desktop-to-Gaming-to-Desktop lifecycle shape and automatic recovery have been observed, but actual daemon survival and GUI relaunch behavior remain for the disposable M4 packaging prototype.
- A sanitized live RomM heartbeat, device pairing, authenticated read-only catalog/save calls, ROM hash lookup and save-summary call have been verified without recording the private server URL; mutating endpoints and runtime negotiation remain untested.
- RomM 5.4.0, Synology Container Manager deployment, absence of a reverse proxy, mutable `rommapp/romm:latest` tag and pairing flow are recorded; the immutable container digest remains unknown.
- Definition defaults and effective system alternatives are recorded for all 12 targets. Unexercised non-priority installations and save behavior remain unvalidated and disabled; M0 approval does not infer compatibility from a default label or candidate root.
- The selected mGBA 0.10-dev core is installed and its three override files contain no save-path directives, but the Bazzite library currently has zero GBA ROMs and no matching `.srm`, `.sav` or companion file; validation requires a disposable/homebrew fixture.
- A real DS save has an approved read-only baseline and uniquely mapped RomM record; disposable extension-only staging has been verified without touching live sources, while emulator-load and network-transfer fixtures remain later tests. No GBA transfer fixture has been prepared.
- The Dolphin GameCube root changed from empty to one USA `.gci` plus one 68-byte `.raw` during M0. The GCI passed basic read-only structure checks, its six-byte identity matches the sole local `.rvz`, and all three local ROM hashes resolve to one RomM `ngc` record. The unreferenced `.raw` cannot be a raw memory-card image and is excluded. Direct RomM listing confirms three distinct `autosave` versions: two `.zip` artifacts created by an unused test client and one `.gci`, which is the explicit latest record. The historical ZIPs are excluded from Dolphin GCI restore candidates but remain visible history. The remote GCI differs from local and the newly paired device has zero sync-baseline entries, so project policy classifies the state as a first-sync conflict. If an unsupported format ever becomes the latest remote head, the adapter must block rather than silently select an older GCI. A disposable runtime-filesystem proof created a byte-identical backup, atomically replaced a working copy using the staged file's inode, revalidated the final GCI structure, left the live source unchanged and removed all temporary files.
- The root RetroArch `.sav` is a manual test artifact excluded from discovery; it is the same size but no longer byte-identical to the live `.srm`, and it matches RomM's retained `Latest` bytes. The `.srm` candidate has a read-only baseline, uniquely hash-resolved RomM game identity, no observed same-stem companion and an effective `melonDS DS` core version of 1.4.0. Controlled extension-only copies in both directions preserved bytes and left both sources unchanged.
