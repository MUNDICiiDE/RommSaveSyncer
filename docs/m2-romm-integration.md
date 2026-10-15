# M2 RomM 5.4 integration and recovery guide

**Specification:** 1.2  
**Compatibility band:** RomM 5.4.x only  
**Target:** Bazzite/Linux

## Implemented boundary

`romm-sync-romm` is the only crate that knows the RomM wire protocol. It provides:

- live `/api/heartbeat` compatibility checks before every operation that can change sync state; versions outside 5.4.x fail closed;
- device authorization with exactly `roms.read`, `platforms.read`, `assets.read`, `assets.write`, `devices.read` and `devices.write`;
- a device-bound credential containing the normalized server origin, device ID, granted scopes, expiry and bearer token;
- negotiation using explicit `autosave` slots and a separate MD5 wire digest;
- upload preflight by visible slot-head hash, followed when needed by `POST /api/saves` with `overwrite=false`, version retention, and no `PUT` fallback;
- downloads with `optimistic=false`, MD5/size validation, and a separate post-commit acknowledgement;
- sync-session completion with explicit completed/failed counts;
- capped exponential retry timing and the durable SQLite retry queue;
- sanitized request traces containing only the HTTP method, route template and status.

The pure planner remains authoritative. A RomM `upload` or `download` operation is only a proposal: the planner compares the last verified common baseline and produces a conflict when both sides differ or the first common baseline is unknown. Server timestamps never select a winner by themselves.

## Transfer ordering

Upload:

1. M1B revalidates adapter, local hashes, remote revision, mount, activity and free space.
2. The adapter stages the exact payload and the client computes RomM's MD5 independently from the internal SHA-256.
3. The transport re-observes the slot immediately before `POST /api/saves`.
4. A matching visible head makes a completed retry a read-only success without another POST.
5. Otherwise the request uses `overwrite=false`; a changed slot is a conflict.
6. The returned ROM, slot and MD5 must match before the common baseline is advanced.

Download:

1. The client fetches with `optimistic=false` and validates MD5 and size before exposing bytes to M1B.
2. M1B stages, revalidates all guards, backs up the current save and atomically replaces the live file.
3. M1B reads the committed file back and verifies it.
4. Only then does `RemoteTransport::confirm_download` call `/downloaded`.
5. If confirmation fails, the valid local commit is preserved, the journal is marked failed, no baseline is advanced, and confirmation is eligible for retry.

RomM exposes MD5 metadata, while M1B uses SHA-256 for stale-input protection. `RomMTransport` therefore fetches an unconfirmed remote head and computes SHA-256 before presenting it to the planner/executor. This costs an extra small save download when a remote revision changes, but it avoids treating two different hash algorithms as comparable.

## Credentials

The planned production path is `$XDG_CONFIG_HOME/romm-autosync/credentials.json`. On Unix the store:

- rejects parent traversal and every existing symlink component;
- requires the parent and credential file to belong to the current effective user;
- enforces parent mode `0700` and file mode `0600`;
- writes a same-directory temporary file, syncs it, atomically renames it and syncs the parent;
- opens with `O_NOFOLLOW` and revalidates schema and the exact scope set;
- redacts the bearer token from `Debug` and never writes it to SQLite, traces, command arguments or environment variables.

Plain HTTP is rejected by default. The user's current direct Synology LAN deployment can be enabled only with an explicit insecure-HTTP setting. That exception must be limited to a trusted private network because bearer credentials and save data are otherwise unencrypted in transit. HTTPS remains the recommended deployment.

## Retry and recovery behavior

| Failure | Automatic behavior | User-visible terminal condition |
|---|---|---|
| Offline, timeout, HTTP 408/429/5xx | Queue the operation and apply capped exponential delay with deterministic per-save spread | Stop after the configured attempt cap |
| Upload response lost after server commit | Read the current slot head; matching MD5 is accepted as the original success | Different head becomes a conflict |
| Upload HTTP 409 | Never overwrite and never fall back to PUT | Conflict requiring explicit resolution |
| Download interrupted before local commit | Discard staging; server baseline is unchanged | Retry from fresh observation |
| Download acknowledgement fails after verified commit | Preserve committed local save; do not advance local baseline | Retry acknowledgement/reconcile |
| Credential revoked/expired or scope changed | Do not retry as an offline error | Re-pair the device |
| RomM version leaves 5.4.x | Block negotiation and all state-changing calls | Upgrade the adapter after compatibility review |
| MD5, SHA-256, size, slot or receipt mismatch | Quarantine/reject; never commit or advance baseline | Integrity/protocol error |

Retry rows contain only game identity, operation, attempt count, deadline and sanitized failure text. Payload bytes are rebuilt from a fresh plan after every restart; stale bytes are never stored for blind replay.

## Automated evidence

The integration suite uses a disposable local HTTP implementation, never the user's server or saves. It verifies:

- exact MD5/slot negotiation and conflict parsing;
- fail-closed version drift;
- a lost upload response reconciles to the matching server head with one upload POST;
- a guarded 409 never causes PUT or overwrite;
- download uses `optimistic=false` and confirmation is explicit;
- M1B confirmation occurs only after the atomic live-file replacement is readable;
- durable retry ordering, attempt advancement, backoff capping and exhaustion;
- traces omit bearer tokens, IDs, query strings and bodies.

Run the target-independent suite with:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

CI runs these commands on Linux. Linux CI also exercises the Unix credential-file mode, ownership and symlink tests.

## Guarded Bazzite qualification harness

`romm-m2-qualify` is a temporary Bazzite/Linux-only milestone tool. The approved M2 fixture is the disposable **Super Mario 64 DS** save. **Kirby Air Ride** is reserved for M3 because its per-game Dolphin GCI structure is an adapter concern, not a generic RomM transport test.

The DS command operates on an isolated copy and never writes the supplied live `.srm` path. Before changing RomM it requires a new workspace, a regular non-symlink `.srm`, an existing remote `.sav` head with different bytes, and no more than seven retained autosave versions. It then:

1. copies and syncs the local `.srm` into the new workspace;
2. downloads and verifies the original remote `.sav` with `optimistic=false`;
3. proves that the pure planner classifies the baseline-free divergence as a conflict;
4. confirms the staged download only after writing and reading it back;
5. establishes that remote revision as the test device baseline;
6. uploads the copied local bytes as `.sav`, verifies the head, and repeats the upload to prove the retry is a read-only no-op;
7. uploads the downloaded original bytes to restore the remote head; and
8. verifies that the live `.srm` remained byte-identical throughout.

The exercise intentionally retains two additional RomM versions: the temporary local revision and the restored original revision. RomM's configured ten-version autocleanup is left enabled, so the harness refuses to start when more than seven versions already exist. The workspace and downloaded original remain on disk as recovery evidence. If execution stops after the first upload but before restoration, do not rerun with a new workspace: preserve the existing workspace and restore `downloaded-remote.sav` through RomM before proceeding.

Successful trusted pushes to `main` and manual CI runs publish a directly downloadable `romm-m2-qualify-linux-x86_64.tar.gz` artifact for 14 days. Pull requests do not publish executables. CI builds it with the pinned Rust 1.99 toolchain, smoke-tests `--help`, and packages the source commit plus a SHA-256 manifest with the executable. This avoids installing Rust or native build dependencies on Bazzite.

Download the artifact from the successful GitHub Actions run onto Bazzite, then extract it into a new private directory and verify the executable before use:

```bash
QUAL_DIR="$HOME/Applications/romm-m2-qualify-$(date +%Y%m%d-%H%M%S)"
mkdir -m 700 "$QUAL_DIR"
tar -xzf "$HOME/Downloads/romm-m2-qualify-linux-x86_64.tar.gz" -C "$QUAL_DIR"
(
  cd "$QUAL_DIR"
  sha256sum -c SHA256SUMS
  cat BUILD-INFO.txt
  ./romm-m2-qualify --help
)
QUAL_BIN="$QUAL_DIR/romm-m2-qualify"
```

`sha256sum` must report `OK`, `BUILD-INFO.txt` must name the intended repository commit, and `--help` must succeed. Preserve `QUAL_BIN` in the same shell for the following commands.

Pair on Bazzite without exposing a bearer token:

```bash
"$QUAL_BIN" pair 'http://ROMM-LAN-HOST:PORT' \
  "$HOME/.config/romm-autosync/credentials.json" \
  --allow-insecure-http
```

The command prints RomM's verification path and one-time user code, waits for approval, and saves the resulting credential with directory mode `0700` and file mode `0600`. It never prints the bearer token. The insecure-HTTP flag is required only for the current direct Synology LAN deployment; it sends the token and save bytes without transport encryption and must not be used across an untrusted network.

After closing RetroArch and obtaining the already-audited numeric RomM ROM ID, run:

```bash
"$QUAL_BIN" nds "$HOME/.config/romm-autosync/credentials.json" ROM_ID \
  "$HOME/Emulation/saves/retroarch/saves/SUPER_MARIO_64_DS.srm" \
  "$HOME/romm-m2-sm64ds-qualification" \
  --allow-insecure-http \
  --confirm-disposable-romm-mutation
```

Replace the placeholder filename with the actual single `.srm` path; quote it when it contains spaces. The output is a sanitized JSON report containing check names and counts, never the server address, device ID, ROM ID/title, filename, hashes, token, authorization code or save bytes. A failed run does not satisfy the milestone gate.

## Remaining disposable RomM gate

The M2 exit gate is not yet claimed. The guarded DS harness covers the real-server bidirectional byte transfer, first-sync conflict, completion, retry deduplication and restoration portions. Before M2 is marked complete, capture the harness report and complete the remaining disposable RomM 5.4.x checks for:

1. device revocation and rejection of the revoked credential;
2. retry after an intentionally lost live response, proving no duplicate version;
3. a competing disposable-device slot change returning 409 and preserving both copies; and
4. reconnect and queued retry after a temporary outage.

Initial upload into a truly empty slot remains an automated protocol fixture rather than part of this selected live-ROM exercise because Super Mario 64 DS already has retained history. If the milestone gate requires that case against the deployed server, create a separate disposable RomM ROM/save fixture instead of deleting this history.

The evidence must record version, route template, status, operation counts, slot, extension, size and whether hashes matched. It must not contain server address, device ID, ROM title/ID, filename, token, authorization code, raw response body or save bytes.
