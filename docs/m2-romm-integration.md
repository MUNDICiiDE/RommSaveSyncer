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
- `POST /api/saves` uploads with `overwrite=false`, version retention, and no `PUT` fallback;
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
4. The request uses `overwrite=false`; a changed slot is a conflict.
5. The returned ROM, slot and MD5 must match before the common baseline is advanced.

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

## Remaining disposable RomM gate

The M2 exit gate is not yet claimed. Before M2 is marked complete, use an isolated RomM 5.4.x test user, device and ROM/save—not a live save—to capture sanitized evidence for:

1. device authorization and revocation;
2. an initial upload into an empty `autosave` slot;
3. an `optimistic=false` download followed by confirmation only after local commit;
4. retry after an intentionally lost response, proving no duplicate version;
5. a competing-device slot change returning 409 and preserving both copies;
6. session completion counts and reconnect after a temporary outage.

The evidence must record version, route template, status, operation counts, slot, extension, size and whether hashes matched. It must not contain server address, device ID, ROM title/ID, filename, token, authorization code, raw response body or save bytes.
