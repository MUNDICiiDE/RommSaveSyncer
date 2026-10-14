# M1A Planner Contract

**Status:** Complete. Local validation and Windows/Linux GitHub Actions passed on 2026-10-14.

## Boundary

`romm-sync-planner` is a pure Rust library. It accepts owned, normalized evidence as immutable input and returns a deterministic ordered plan. It does not read files, inspect processes, query RomM, access a database, read the clock, generate random values, or mutate global state.

Discovery and execution are separate boundaries:

1. A future discovery layer collects local, RomM, mount, adapter, and activity evidence.
2. Adapter contracts normalize that evidence into `ReconciliationInput` values.
3. The planner classifies each game/slot and proposes an action.
4. A future M1B executor revalidates every attached precondition before carrying out an upload or download.

No M1A code performs step 1 or step 4.

## Supported model

The domain model represents all twelve target platforms: DS, GBA, GameCube, NES, SNES, Nintendo 64, Wii, Nintendo 3DS, PlayStation, PlayStation 2, PSP, and Switch. Representation does not enable an adapter. Each adapter is independently marked `Validated`, `PendingValidation`, `Unsupported`, or `TemporarilyUnsafe`.

The initial validation order remains:

1. Nintendo DS with the `.srm`/`.sav` compatibility adapter.
2. Game Boy Advance through RetroArch mGBA.
3. Dolphin GameCube game-specific GCI.

The model also distinguishes single files, multi-file groups, game-specific memory cards, directory trees, shared memory cards, and unknown layouts.

## Decision precedence

The planner uses a stable fail-closed order:

1. Reject adapter/platform mismatches and adapters that are not validated.
2. Defer missing or ambiguous game identity.
3. Defer missing or unmounted roots.
4. Defer incomplete groups and reject malformed groups.
5. Defer unavailable RomM observations and reject malformed observations.
6. Reject invalid baselines and incompatible, unknown, or mismatched formats/slots.
7. Compare normalized content against the last common baseline.
8. Reject unsupported transfer directions.
9. Before proposing a mutation, require confirmed inactive activity.
10. Attach local, remote, adapter, path, mount, activity, and free-space preconditions.

Static conflicts and no-op results remain observable while a game is active because they do not write data. Activity blocks every upload or download proposal.

## Reconciliation rules

| Local | Remote | Baseline | Outcome |
|---|---|---|---|
| Same content | Same content | Any | No-op / in sync |
| Different | Different | None | Conflict; timestamps are not a tie-breaker |
| Changed | Baseline content | Known | Upload |
| Baseline content | Changed | Known | Download |
| Changed | Changed | Known | Conflict |
| Present | Absent | None | Initial upload |
| Absent | Present | None | Initial download |
| Present | Deleted/absent after baseline | Known | Preserve local and defer |
| Absent after baseline | Present | Known | Conflict |
| Both absent | None | No-op |
| Both absent after baseline | Known | Defer |

Remote deletion never produces local deletion. M1A has no delete action.

## Stable actions and reasons

Actions are `upload`, `download`, `no-op`, `conflict`, `defer`, `unsupported`, and `error`. `ReasonCode::as_str()` is the stable machine representation used by future CLI, IPC, logs, notifications, and history.

Every upload/download includes:

- the complete expected adapter descriptor, including its ID, revision, validation state, platform, save model, compatibility, slot, and transfer capabilities;
- logical/resolved root and mount identity;
- expected local absence or normalized group content, integrity, and member hashes;
- expected remote absence or content and revision;
- required `Inactive` activity state; and
- minimum free bytes needed for staging.

M1B must re-observe these values immediately before mutation. A mismatch rejects the plan and triggers replanning or a visible blocked/conflict state.

## Tests and proof obligations

The automated suite covers the required decision matrix, all twelve platform values, conservative activity classification, normalized group member ordering, all permutations of representative discovery order, duplicate-key ambiguity, input immutability, and a cross-product invariant that a mutation exists if and only if revalidation preconditions are attached.

CI runs formatting, tests, and warning-free Clippy checks on Linux and Windows. These tests use synthetic snapshots only; no real save path is opened.

## Deferred to M1B

The following are deliberately unresolved until the guarded executor milestone:

- the SQLite schema and migration mechanism;
- how discovery revisions are persisted and invalidated;
- the concrete Bazzite process/session probes behind activity evidence;
- exact backup directory layout and retention accounting;
- atomic-write primitives and durability semantics per filesystem;
- the CLI serialization format and versioned executor protocol; and
- free-space overhead beyond the planner's minimum payload size.
