# M1B Guarded Executor Contract

**Status:** Implementation complete locally on 2026-10-14. Windows validation passes; the first Windows/Linux CI run for this change set is pending commit and push.

## Boundary

M1B adds local discovery, durable state, backup/recovery, and a guarded executor around the pure M1A planner. It does not add a live RomM client, production emulator-process detection, a background service, or any enabled emulator adapter. Those remain gated by M2, M3, and M4.

The only executable delivered in M1B is deliberately read-only:

```text
romm-sync dry-run <discovery-manifest.json>
romm-sync status <state.db>
```

`dry-run` reads only the explicitly named files, performs two observations separated by 250 ms, hashes stable regular files with SHA-256, invokes the M1A planner, and emits versioned JSON with stable action/reason codes. `status` opens SQLite in read-only mode. Neither command can invoke the executor.

## Discovery manifest

Discovery schema version 1 is strict: unknown JSON fields, unsupported values, empty item sets, duplicate platform/ROM/slot keys, absolute member paths, and path traversal are rejected before scanning. The platform field accepts all twelve inventory slugs: `nds`, `gba`, `ngc`, `nes`, `snes`, `n64`, `wii`, `3ds`, `psx`, `ps2`, `psp`, and `switch`. Representation does not enable an adapter.

Example using a disposable fixture:

```json
{
  "schema_version": 1,
  "items": [
    {
      "platform": "nds",
      "rom_id": "fixture-rom-id",
      "slot": "autosave",
      "adapter": {
        "id": "nds-fixture",
        "revision": "1",
        "status": "validated",
        "save_model": "single_file",
        "compatibility": "compatible",
        "can_upload": true,
        "can_download": true
      },
      "identity": "verified",
      "activity": "inactive",
      "root": "/path/to/disposable/save-fixture",
      "files": ["game.srm"],
      "remote": { "state": "absent" },
      "baseline": { "state": "none" }
    }
  ]
}
```

The remote and activity values are normalized observations supplied by future M2/M4 implementations; the manifest is a test and inspection interface, not a production trust boundary.

## Execution protocol

The executor accepts only M1A `upload` or `download` plans with complete mutation preconditions. Before any operation and again immediately before a mutation it verifies:

- adapter identity, revision, platform, save model, compatibility, slot, and capabilities;
- logical/resolved save root and mount identity;
- complete local group membership, member sizes, member SHA-256 hashes, and group hash;
- remote content hash and revision, or confirmed absence;
- confirmed inactive activity (unknown fails closed);
- minimum free space; and
- a validated one-to-one local/remote member mapping.

Each game/platform/slot has a cross-process nonblocking file lock. A stale guard, ambiguous/non-mutating plan, unsafe mapping, or busy lock returns a stable rejection code and writes history where a state store is available. It never silently upgrades a stale plan.

`RemoteTransport` is an abstract conditional transport. The M2 implementation must re-observe revisions and enforce the expected remote state during upload/download; returning a receipt is not enough to bypass executor verification.

## Download and rollback protocol

Downloads are staged beneath the destination filesystem. The payload manifest, size, and group SHA-256 are verified before the live path is touched. Existing files receive a copied and verified routine backup, then the journal advances through `prepared`, `staged`, `backed_up`, and `replacing`. Each member is replaced from the same filesystem and the completed live group is re-hashed before the baseline is committed.

On Bazzite/Linux, a single-file replacement uses same-filesystem `rename`, which is atomic. A multi-file group cannot be atomically committed as one filesystem operation; it is protected by a durable journal, a complete verified group backup, fail-fast replacement, and full-group rollback. Windows development builds use a journaled rename/restore fallback and are tested for rollback, but do not claim a single primitive equivalent to Linux replacement.

Routine backups retain ten versions per game by default. A configurable routine-storage cap may prune older routine versions, but never the backup protecting the current transaction. Conflict archives live in a separate tree and are excluded from automatic routine pruning. Backups are verified at creation and every member is preflighted again before recovery changes any live file.

## State and restart recovery

SQLite schema version 1 contains devices, game mappings, local save groups/hashes, remote observations, synchronization baselines, retry work, plans/items, conflicts, immutable history, and the execution journal. Writable opens use WAL mode, full synchronization, transactional migrations, a WAL-aware SQLite online backup before migration, and an integrity check. Status opens do not migrate or write.

Startup recovery processes only non-terminal journals. Recovery is deferred while activity is active/unknown, the root is missing, or mount identity changed. Interrupted downloads either restore the verified complete backup or remove only files that the journal proves were newly created. Interrupted uploads never alter local files; they terminate with an audit reason requiring the future RomM layer to re-observe remote state before replanning. Re-running recovery after success is idempotent.

## Stable executor rejection codes

`non_mutating_plan`, `missing_preconditions`, `lock_busy`, `invalid_mapping`, `adapter_changed`, `root_changed`, `mount_changed`, `local_changed`, `remote_changed`, `activity_active`, `activity_unknown`, `insufficient_space`, `payload_invalid`, `commit_failed`, and `recovery_deferred`.

## Verification evidence

The local suite uses only temporary directories and synthetic bytes. It covers read-only CLI behavior, transactional migration and migration backup, verified upload/download, stale local and remote observations, mount replacement, unknown/active activity, low space, truncated payloads, remote/activity races, failed multi-file replacement and rollback, interrupted download/upload recovery, corrupted backup refusal, cross-process lock contention, backup rotation/cap behavior, protected conflicts, ambiguous identity, and non-executable conflicts.

The required local commands are:

```text
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

GitHub Actions runs the same workspace tests and Clippy checks on Windows and Linux. No test references or modifies a real save.

## Decisions deliberately deferred

- RomM 5.x endpoint selection, conditional request semantics, authentication, retries, and device registration: M2.
- Concrete DS, GBA, and Dolphin GCI mappings and format-specific validation: M3.
- Concrete Bazzite emulator/game activity probes and service lifecycle: M4.
- Production state, lock, staging, and backup locations plus installer ownership: M4 packaging prototypes.
- A default byte cap for routine backups: no cap is imposed yet; the engine supports one and always preserves the in-flight rollback copy.
