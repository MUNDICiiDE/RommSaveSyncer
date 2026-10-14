# ADR-0001: Tooling, IPC, GUI and Bazzite Packaging Direction

**Status:** Accepted for implementation planning; physical package qualification remains an M4 gate

**Date:** 2026-10-14

**Decision required by:** M0 exit gate

## Context

RomM AutoSync requires:

- A continuously running user-scoped daemon independent of either GUI.
- Safe filesystem, hashing, SQLite, HTTP and recovery primitives.
- A KDE desktop dashboard and tray.
- A fullscreen controller-friendly Gaming Mode dashboard.
- Optional Decky integration that cannot be a daemon dependency.
- Installation and updates compatible with Bazzite's image-based system.

Bazzite's documentation recommends Flatpak for graphical applications, Homebrew for command-line tools, containers/Quadlet for services when appropriate, AppImage for portable graphical applications, and `rpm-ostree` layering only as a last resort.

Primary reference: [Bazzite software installation guidance](https://docs.bazzite.gg/Installing_and_Managing_Software/software-intro/)

## Decision drivers

1. Preserve access to the actual EmuDeck/emulator save paths and resolved symlink targets.
2. Support a `systemd --user` daemon across Desktop and Gaming Mode transitions.
3. Avoid system package layering and unsupported immutable-root changes.
4. Keep the daemon independent from KDE, gamescope and Decky.
5. Support secure user-scoped IPC and credentials.
6. Provide reproducible installation, update, rollback and uninstall.
7. Keep planner and core logic independently testable in CI.
8. Minimize duplicated logic across desktop and controller interfaces.

## Candidate implementation stacks

| Candidate | Advantages | Risks/questions | M0 state |
|---|---|---|---|
| Rust daemon/core + Tauri 2/React UI | Strong filesystem/concurrency types, SQLite/HTTP ecosystem, one responsive desktop/controller UI, compact native backend; target has WebKitGTK 4.1 and libappindicator3 | Tray and gamescope/controller behavior still require physical qualification | **Selected** |
| Rust daemon/core + native Qt UI | Mature KDE/tray integration and controller-capable widgets | Adds a second language/binding layer and a separate UI architecture without resolving Gaming Mode qualification | Rejected for v1; fallback only if Tauri prototype fails |
| Python daemon + Qt UI | Fast prototyping and mature libraries | Runtime bundling and long-running safety/type discipline are weaker for the guarded executor | Rejected for v1 |
| .NET daemon + Avalonia UI | Strong tooling and one ecosystem | Adds an unverified target runtime/package footprint while the Rust/Tauri prerequisites already match the host | Rejected for v1 |

Select a Rust workspace for the pure planner, adapters, guarded executor and daemon, plus a Tauri 2 shell with a React/TypeScript interface shared by the Desktop dashboard/tray and fullscreen controller presentation. The WebView never performs authenticated RomM or save-file operations.

## IPC candidates

| Candidate | Advantages | Risks/questions |
|---|---|---|
| Unix-domain socket with authenticated local protocol | User-scoped permissions, transport-agnostic UI clients, straightforward testing | Protocol/versioning and peer verification must be designed |
| D-Bus user session | Native desktop service model and introspection | Availability/behavior across Desktop and Gaming Mode sessions; daemon should not depend on a graphical bus |
| Loopback HTTP | Easy web UI integration | Port ownership, CSRF/origin/authentication and accidental exposure |

Select a versioned, size-bounded request/response protocol over a Unix-domain socket at `$XDG_RUNTIME_DIR/romm-autosync/agent.sock`. The daemon creates a mode-`0700` parent and mode-`0600` socket, rejects unexpected peer UIDs using Linux peer credentials, never returns the RomM token and exposes only explicit commands. The Flatpak GUI receives access only to `xdg-run/romm-autosync`. D-Bus remains available for desktop integration where useful but is rejected as the core protocol because graphical services and environment change across modes. Loopback HTTP is rejected because it adds port, origin and CSRF exposure with no local benefit.

## Packaging candidates

| Candidate | Daemon fit | GUI fit | Key audit question |
|---|---|---|---|
| User-local bundle under XDG data directories plus `systemd --user` unit | **Selected for daemon** | Not selected for primary GUI | Signed prebuilt artifact, side-by-side rollback and service lifecycle require M4 proof |
| Flatpak GUI plus separately packaged user daemon | **Selected split** | **Selected for GUI** | Runtime-directory socket and coordinated compatibility/update behavior require M4 proof |
| Single Flatpak containing GUI and background component | Rejected | Strong | Flatpak does not export host systemd services and a background component would require broad save/process access |
| AppImage GUI plus user-local daemon | Good | Portable fallback | Less sandbox enforcement and independent update/tray integration; retain only if Flatpak prototype fails |
| Homebrew package | Possible fallback | Weak | Available on target, but does not solve GUI distribution or coordinated rollback |
| Quadlet/container service | Rejected | Separate GUI required | Unnecessary host-path, user-session and deployment complexity |
| `rpm-ostree` layered RPM | Rejected | Rejected | Violates the no-layering direction and requires image deployment/reboot coupling |

## Selected packaging and update direction

Ship the Rust daemon as a signed, prebuilt user-local bundle beneath `$XDG_DATA_HOME/romm-autosync/releases/<version>/`, selected by an atomically updated `current` link and run by a `systemd --user` service attached to non-graphical `default.target`. Do not enable lingering by default. The daemon owns filesystem discovery, activity detection, credentials, RomM networking, SQLite and all mutations.

Ship the Tauri 2 GUI as a Flatpak with Wayland, fallback-X11, DRI, shared IPC, KDE StatusNotifier access and only the `xdg-run/romm-autosync` filesystem grant needed for the daemon socket. It receives no `host`/`home` filesystem or network permission. Desktop launch provides the tray/dashboard context; a Steam non-Steam shortcut launches the same Flatpak in fullscreen controller mode. Optional Decky support remains a client, never a daemon dependency.

Build both artifacts in CI; the Beelink is a deployment/qualification target and requires no compiler toolchain. Publish a signed release manifest containing daemon/UI versions, hashes and compatible IPC schema ranges. Install daemon releases side by side, switch only after health/schema checks, retain the previous compatible release for rollback, and never roll back the state database destructively. Let Flatpak manage GUI version objects and rollback; the GUI must reject an incompatible daemon schema with recovery guidance rather than attempting an unsafe partial update.

The M4 prototype must demonstrate:

- Clean user-scoped install and uninstall.
- No write to the immutable system image.
- Daemon startup and recovery.
- Desktop/Gaming Mode transition survival.
- Access only to approved save/config paths.
- Tray and controller-dashboard launch behavior.
- Update and rollback of both daemon and UI without losing state.

## Credential storage candidates

- **Selected for v1:** owner-only `$XDG_CONFIG_HOME/romm-autosync/credentials.json` in a mode-`0700` directory, with mode `0600`, created and read only by the daemon using symlink-safe and atomic operations.
- Optional Desktop integration: Secret Service-compatible keyring, never required for daemon startup or synchronization.
- Deferred: user-scoped encrypted `systemd-creds`, because the valid scoped key types depend on host credential state and this target has no initialized host key.

## Required M0 evidence

- Bazzite version, desktop and Gaming Mode session behavior.
- Availability of required native runtimes/libraries.
- Flatpak IDs and permissions for selected emulators.
- Whether a user daemon can access approved host paths without weakening GUI sandboxing.
- Tray behavior under KDE.
- Fullscreen/controller behavior under gamescope.
- IPC availability across session transitions.
- Keyring/credential access while no desktop session is active.
- Update/rollback mechanism suitable for signed releases.

The session, runtime, sandbox and credential questions have enough evidence to select the direction above. Tray rendering, Steam shortcut/controller focus, runtime-socket access from the final Flatpak, actual daemon survival and coordinated update/rollback remain explicit M4 packaging-prototype acceptance tests rather than unverified M0 claims.

## Observed M0 evidence

The Desktop Mode baseline is KDE on Wayland. The per-user systemd manager is running; `default.target`, `graphical-session.target` and `plasma-workspace.target` are loaded and active. `gamescope-session.target` is installed/loaded but inactive in Desktop Mode. Plasma shell is present, gamescope is absent, Steam and helper processes remain present, and the manager environment contains Wayland, display and desktop variables. User lingering remains disabled.

After switching the local console to Gaming Mode, gamescope replaced Plasma while a Wayland session, Steam processes, `default.target` and `graphical-session.target` remained. `plasma-workspace.target` became inactive. `gamescope-session.target` remained inactive despite the live gamescope process, so it is not a reliable deployment-specific lifecycle hook. The shared user manager remained queryable from SSH, but its imported `WAYLAND_DISPLAY` and `DISPLAY` variables disappeared while `XDG_CURRENT_DESKTOP` remained. The manager reported `degraded` because eight Plasma/KDE services and one Flatpak application service were failed; no failed unit belonged to gamescope, Steam, portal, audio, D-Bus, mounts, sockets or targets.

After returning to Desktop Mode without resetting units, gamescope was absent, Plasma and its workspace target were active, the user manager returned to `running`, all nine failures cleared, and its Wayland, display and desktop variables were restored. The observed round trip therefore recovered automatically, but it did not exercise a RomM AutoSync service. The architectural consequence is firm even while the packaging choice remains proposed: the daemon must attach to a non-graphical user lifecycle and must not depend on Plasma, gamescope, their session targets or manager-wide graphical variables. Desktop tray and Gaming Mode/controller clients must be launched in their respective graphical contexts and connect to the daemon over user-scoped IPC. M4 must still prove service survival and client reconnection with disposable packages.

RetroArch, Dolphin, standalone melonDS and PPSSPP are installed as Flatpaks. All four report shared IPC/network access, `devices=all` and broad host filesystem access; none reports a system-bus policy. RetroArch, Dolphin and PPSSPP can access the gamescope runtime socket read-only, while the installed Dolphin build declares X11 rather than Wayland display access. Each application's user override consists only of `filesystems=host;` and `shared=network;` in its context; no override environment or bus-policy entry was reported. These emulator permissions explain why EmuDeck-managed applications can reach their dispersed save paths, but they are not an acceptable permission baseline for RomM AutoSync. The evidence strengthens the split-package direction: keep filesystem, credentials and RomM networking in a user-local daemon and give a separately launched GUI only narrowly scoped IPC and graphical permissions.

The target has no Rust, Cargo, Node.js, pnpm or Flatpak Builder executable on the SSH `PATH`; Podman, Distrobox and Homebrew are present. Release installation must therefore consume prebuilt, reproducible artifacts and must not compile on the Beelink. The native linker cache contains WebKitGTK/JavaScriptCoreGTK 4.1, `libappindicator3`, `libsecret-1` and SQLite 3, but not WebKitGTK 4.0 or `libayatana-appindicator3`. A WebKit-based GUI and native tray are plausible, but only an M4 package prototype can establish exact ABI compatibility and eliminate undeclared host dependencies.

In Desktop Mode, `secret-tool` is installed and `org.freedesktop.secrets` is owned by KDE's `ksecretd`. The bus associates that process with the overall per-user manager rather than a dedicated service unit; the related units are a transient KWallet desktop-portal service and static `plasma-kwallet-pam.service`. The Secret Service name was not listed as bus-activatable. A settled Gaming Mode observation—with gamescope and Steam present and Plasma absent—found no `org.freedesktop.secrets` owner while the user manager remained reachable. Secret Service is therefore rejected as the sole unattended credential backend. The remaining decision is between a user-scoped encrypted `systemd-creds` credential, if the deployed version and lifecycle support it, and a mode-`0600` user credential file. Secret Service may be offered only as an optional Desktop convenience, never as a dependency for sync.

The deployed `systemd-creds` is version 259, advertises user-scoped encryption and UID binding, and reports a complete TPM2 capability stack. The first disposable proof selected `--user --with-key=tpm2`; systemd rejected that combination before service delivery because TPM-only is not available in UID-scoped mode. The wrapper's top-level `exit 1` also closed the interactive SSH shell after the failure, which is a test-script defect and must not be repeated. No persistent service or credential was created, and the runtime cleanup trap executed. A follow-up check found no `/var/lib/systemd/credential.secret`. Initializing system-wide host credential state is outside M0 and unnecessary for this application, so the scoped host/host+TPM branches are deferred.

The v1 decision is an owner-only credential file under `$XDG_CONFIG_HOME/romm-autosync` (or the standard `~/.config` fallback): directory mode `0700`, file mode `0600`, no symlinks, no group/world access, atomic creation/replacement and owner/type/mode revalidation on every load. Only the daemon handles the pairing response and authenticated requests. The WebView receives status but never token material; command-line arguments, environment variables, logs, exports and SQLite must not carry the token. Disconnect must revoke/delete the RomM device credential before unlinking the local file where possible. This design protects against other accounts and offline casual disclosure but not malicious code already running as the same user; that boundary is explicit rather than implied encryption.

Primary references: [systemd credential tool](https://github.com/systemd/systemd/blob/main/man/systemd-creds.xml) and [`LoadCredentialEncrypted=`](https://github.com/systemd/systemd/blob/main/man/systemd.exec.xml)

## Decision

Use Rust for the planner/core/executor/daemon, Tauri 2 with React/TypeScript for both graphical presentations, a versioned peer-checked Unix-domain socket for local IPC, a signed user-local systemd daemon bundle, and a least-privilege Flatpak GUI. Use the owner-only credential file described above. Build in CI and use signed side-by-side daemon releases plus Flatpak versioning for update and rollback. M4 must validate every physical packaging assumption before release hardening.

Primary feasibility references: [Tauri Linux prerequisites](https://v2.tauri.app/start/prerequisites/), [Tauri Flatpak distribution](https://v2.tauri.app/distribute/flatpak/), [Flatpak sandbox model](https://docs.flatpak.org/en/latest/basic-concepts.html), [Flatpak `xdg-run` filesystem permission](https://docs.flatpak.org/en/latest/flatpak-command-reference.html) and [Flatpak suitability limits](https://docs.flatpak.org/en/latest/introduction.html).
