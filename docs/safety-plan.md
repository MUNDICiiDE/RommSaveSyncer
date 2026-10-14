# M0 Safety, Fixture and Rollback Plan

**Specification:** 1.2

**Audit date:** 2026-10-14

**Status:** Approved for post-M0 development planning on 2026-10-14; no live transfers, adapter enablement or system integration authorized

## Safety invariants

1. Never silently lose progress.
2. Never modify a save while its game or emulator is active or activity is unknown.
3. Never execute a stale plan.
4. Never treat a missing mount or changed symlink target as an empty save collection.
5. Never infer game identity solely from a similar filename or title.
6. Never partially replace a multi-file save group.
7. Never automatically choose a winner when both sides changed from the last common baseline.
8. Never propagate deletion automatically in v1.
9. Never report a check, attempted transfer or served download as a verified synchronization.
10. Never retain credentials, authorization headers, ROMs, BIOS files or unsanitized personal paths in repository artifacts.
11. Never silently skip an incompatible current remote slot head. Unsupported historical formats may be excluded from an adapter's restore candidates, but an unsupported latest head must block or conflict rather than selecting an older compatible version automatically.

## M0 permitted actions

- Read approved version and configuration metadata.
- Resolve approved paths, symlinks and mount identities.
- Record filenames, sizes, timestamps and hashes only within the agreed scope.
- Copy explicitly approved save samples into a disposable working directory.
- Create synthetic fixtures.
- Exercise conversion or rename assumptions only on disposable copies.
- Observe current systemd/session state without changing it.
- Research and document packaging approaches without installing them.

## M0 prohibited actions

- Uploading or downloading a live save.
- Replacing, renaming, deleting or editing a live save.
- Enabling automatic synchronization.
- Installing or enabling a daemon, user service, launch hook or Decky component.
- Enabling/disabling user lingering.
- Changing emulator, RetroArch, EmuDeck, ES-DE, Steam or Flatpak configuration.
- Copying real ROMs, BIOS files, credentials or raw configuration dumps into Git.

## Evidence and redaction policy

Every captured fact should include target, timestamp, command/source, sanitized result and evidence state. Before committing:

- Replace usernames and hostnames with stable labels such as `<bazzite-user>` and `<romm-host>`.
- Replace private addresses and mount names where they are not required for the design.
- Remove tokens, cookies, query credentials and authorization headers entirely rather than masking only part of them.
- Prefer configuration excerpts over whole files.
- Store hashes for approved fixture data, not for ROM or BIOS content.

## Credential handling decision

- The v1 daemon stores the normalized server origin, device ID, exact granted scopes, expiry and device token in `$XDG_CONFIG_HOME/romm-autosync/credentials.json`, or the standard `~/.config` fallback.
- Create the parent directory with mode `0700` and the credential file with mode `0600`; reject symlinks, non-regular files, wrong ownership and any group/world permission bits.
- Use exclusive creation for first write and same-filesystem atomic replacement for refresh. Revalidate owner, type and mode before every read.
- Pairing and refresh responses flow directly from the daemon's authenticated client to this store. The WebView, command-line arguments, environment variables, logs, exports and SQLite never receive token material.
- Do not include the plaintext credential file in automatic configuration backups. A user-directed migration must warn that it contains a bearer credential.
- On disconnect, revoke or delete the RomM device credential first where possible, then unlink the local file. Do not claim secure erasure on SSD/Btrfs storage.
- Secret Service may be added as a Desktop-only convenience but can never be required for Gaming Mode synchronization. `systemd-creds` is deferred because TPM-only is invalid for UID-scoped credentials and this target has no initialized host credential key.
- The mode-`0600` design protects against other OS accounts, not malicious processes already running as the same user. Document this threat boundary and preserve least-privilege RomM scopes and revocability.

## Fixture policy

Preferred fixture order:

1. Synthetic format-valid fixture created specifically for tests.
2. Emulator-generated disposable save for a legally available test/homebrew ROM.
3. Explicitly approved copy of a real save, sanitized and kept outside Git unless its contents are confirmed safe to publish.

Every fixture manifest must record:

- Source category and approval.
- Platform, emulator/core and version.
- Game identity fields needed by the adapter.
- Expected file group and relative paths.
- Size and SHA-256 for local integrity.
- Expected RomM wire hash when applicable.
- Whether bytes may be redistributed in the repository.

## Planner/executor safety boundary

The M1A planner will consume immutable observations and produce deterministic proposed actions. It performs no I/O. Every mutation proposal must carry preconditions describing the exact local hash/group, remote revision/baseline, path/mount identity and required inactive state.

The M1B executor will re-observe those preconditions immediately before mutation. If any precondition changed, it must write nothing and instead replan or report a blocked/conflict state.

## Future local replacement sequence

This sequence is a design requirement, not authorized M0 behavior:

1. Acquire the per-game lock.
2. Revalidate activity, mount/path identity, source hashes/group and remote baseline.
3. Check free space and backup destination.
4. Stage downloaded/generated bytes on the destination filesystem.
5. Validate size, protocol hash, local SHA-256 and adapter completeness.
6. Create and verify a recoverable backup of the whole existing group.
7. Atomically replace the whole group where possible; otherwise use a journaled adapter-specific transaction.
8. Re-read and verify committed bytes.
9. Record the audit event and update the local baseline transactionally.
10. Confirm the download/session to RomM only after successful local verification.

Any failure leaves the original or verified backup recoverable and must not be recorded as synchronized.

## Backup and rollback requirements

- Default routine retention target: 10 versions per game, subject to the unresolved disk-cap decision.
- Conflict copies are protected from routine pruning.
- Backups preserve the complete adapter-owned group and sufficient metadata to restore it.
- A backup is not valid until every expected member is readable and hashed.
- Restore uses the same activity, mount, space, staging, revalidation and verification protections as download.
- Database migrations preserve the previous database until the migrated database passes integrity checks.

## M0 rollback plan

M0 performs no live mutations, so its rollback is limited to deleting disposable copies and reverting documentation changes. Any emulator-generated fixture must live in a dedicated disposable directory, not an active emulator save root.

If an accidental live mutation is suspected:

1. Stop all audit activity without changing additional files.
2. Keep the emulator closed.
3. Preserve the affected directory and metadata as-is.
4. Do not run cleanup or synchronization tools.
5. Compare against the last known backup/copy and obtain explicit approval before restoration.

## Approval gates

| Gate | Required before |
|---|---|
| M0 environment scope approval | Reading private target configuration |
| M0 completion approval | Beginning M1A implementation |
| Dry-run approval | First live upload or download |
| Adapter approval | Enabling automatic writes for each platform/configuration |
| System integration approval | Enabling lingering, installing services/hooks or Decky components |
| Release approval | Shipping automatic synchronization |

## Unresolved safety decisions

- Routine-backup disk cap and pruning thresholds.
- Atomic multi-file transaction design where directory exchange is unavailable.
- Treatment of remote deletion beyond preserving locally and alerting.
- Retention and encryption expectations for local history and backups.
- Exact stable-file interval and maximum wait per adapter.
