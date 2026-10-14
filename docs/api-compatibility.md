# M0 RomM and Client API Compatibility Audit

**Specification:** 1.2

**Audit date:** 2026-10-14

**Status:** M0 approved 2026-10-14; RomM 5.4.0 contract and authenticated read-only behavior verified, with mutating negotiation/transfer tests deferred to disposable M2 integration

## Scope

This document separates upstream protocol documentation from behavior verified against the user's deployed RomM. It contains no credentials, tokens, cookies or private server addresses.

## Public RomM protocol baseline

The currently reviewed upstream reference is the RomM 5.4 device sync protocol. It documents these primitives:

- A registered device identified by a UUID.
- A negotiate/complete sync session.
- Per-save operations: `upload`, `download`, `conflict`, `delete` and `no_op`.
- Per-device sync baselines used to distinguish ordinary updates from conflicts.

Primary reference: [RomM 5.4 Device Sync Protocol](https://docs.romm.app/5.4.0/developers/device-sync-protocol/)

The deployed server version was verified from the RomM UI as 5.4.0 on 2026-10-14. The public reference therefore matches the deployed semantic version, but the live endpoint schemas and enabled features still require safe verification.

The user supplied the public [RomM 5.4.0 OpenAPI document](https://romm.curious-coders.com/openapi.json). Its API title and version match the deployed server, and its sync-relevant paths and component schemas were inspected on 2026-10-14. This is authoritative schema evidence for the matching release, but it is not proof that every documented error path behaves identically on the private deployment.

An unauthenticated live `GET /api/heartbeat` response independently reported `SYSTEM.VERSION` as `5.4.0`, `FILESYSTEM.TITLE_ID_EXTRACTION_ENABLED` as `true`, and `DEVICE_INSTALL.ENABLED` as `true`. The response did not contain a dedicated device-sync or snapshot/channel capability field, so endpoint support must not be inferred from heartbeat alone.

The deployment uses Synology Container Manager without a reverse proxy. The container image is `rommapp/romm:latest` and auto-restart is enabled. The RomM UI visibly exposes **Client API Tokens** and **Devices** sections. This confirms the management surfaces exist, but not yet the exact pairing flow, available token scopes or endpoint behavior.

Because `latest` is a mutable tag, the verified 5.4.0 compatibility band cannot be inferred permanently from the configured image name. The client must read the live server version/capabilities, invalidate cached compatibility conclusions when they change, and refuse mutating sync operations until the new band is supported. M0 does not change or pin the server image.

## Authentication and pairing

RomM documents bearer Client API Tokens and normal web sessions. A companion application should use a dedicated, narrowly scoped Client API Token rather than store an account password or browser session.

Documented endpoint scope requirements relevant to save sync include:

| Endpoint | Documented scopes |
|---|---|
| `POST /api/devices` | `devices.write` |
| `POST /api/sync/negotiate` | `assets.read`, `devices.read` |
| `POST /api/sync/sessions/{id}/complete` | `devices.write` |
| `POST /api/saves`, `PUT /api/saves/{id}` | `assets.write` |
| `GET /api/saves/{id}/content` | `assets.read` |
| `POST /api/saves/{id}/downloaded` | `devices.write` |

The inspected OpenAPI contract also exposes device authorization:

1. `POST /api/auth/device/init` is open and rate-limited. The client supplies its stable device identifier, display name, client name/version, platform and requested scopes.
2. RomM returns a device code, user code, relative verification path, expiry and polling interval.
3. The signed-in user reviews the pending request and approves an explicit subset of scopes in RomM.
4. `POST /api/auth/device/token` is an open, rate-limited polling endpoint that returns the device-bound access token, device ID, granted scopes and optional expiry only after approval.

The minimum end-to-end scope set for v1 is `roms.read`, `platforms.read`, `assets.read`, `assets.write`, `devices.read` and `devices.write`. The two catalog read scopes are required to map a local ROM identity safely to the `rom_id` and platform expected by negotiation; no ROM or platform write scope is required. Basic authentication is present in the OpenAPI alternatives but is rejected for the client because it would require retaining account credentials.

The device-authorization flow was exercised against the deployed RomM server on 2026-10-14. The user approved the request in RomM, polling returned both an access token and device ID, the token reported exactly the six requested scopes, and an expiry was present. No token, device ID, user identity or pairing code was copied into the project or audit transcript.

Authenticated read-only calls using that temporary device token succeeded for devices, platforms, saves and per-ROM metadata. The paired test device was visible. All 12 target platform slugs were present. The current user had 43 `autosave` records distributed as: 3DS 7, GBA 11, N64 3, Nintendo DS 9, GameCube 9, PSP 2, SNES 1 and Wii 1. Platforms absent from this count had no returned `autosave` record; this is not evidence that their RomM libraries are absent.

All nine Nintendo DS records used `.sav`, carried a content hash and carried origin-device metadata. Their observed size set was 512, 8,192, 262,144 and 524,288 bytes. The response contained zero DS device-sync entries for the newly paired test context. Five DS records shared the local candidate's 8,192-byte size, but a local-only comparison found zero matching MD5 values. The current local bytes therefore differ from every returned DS `autosave`. The save stem identifies exactly one local DS ROM but matches two remote DS records, so filename matching is ambiguous. A transient read-only hash lookup of the sole matching local `.nds` resolved CRC, MD5 and SHA-1 consistently to one RomM DS ROM. Both candidate saves belong to that ROM and `autosave` slot, use `.sav`, and have distinct content hashes. The RomM UI identifies them as two retained versions and marks one `Latest`. This resolves game identity and explains the duplicate rows, but it does not establish a safe winner against the different local content.

Primary reference: [RomM 5.4 Client API Tokens](https://docs.romm.app/5.4.0/developers/client-api-tokens/)

### M0 decisions

- Never ask the user to paste a full token into the repository, chat transcript or shell history.
- Use RomM device authorization/pairing for normal setup, subject to one disposable deployed-server verification.
- Request only `roms.read`, `platforms.read`, `assets.read`, `assets.write`, `devices.read` and `devices.write`; do not request ROM/platform write, profile, user, task or log scopes.
- Store the returned device ID with the credential because the download-confirmation schema requires it even though negotiation can infer a device from a device-bound token.
- Store the normalized server origin, device ID, granted scopes, expiry and token in the daemon-owned credential file selected by ADR-0001: mode-`0700` parent, mode-`0600` file, symlink-safe atomic writes and owner/type/mode checks on load. Never expose the token to the WebView, command line, environment, logs, exports or SQLite.

## Device registration

The documented `POST /api/devices` request includes name, platform, client, client version, hostname, MAC address, sync mode and capabilities. Registration is documented as idempotent when an existing device matches by MAC address, or by hostname and platform.

The preferred device-authorization approval flow creates the device and bound token together. Direct `POST /api/devices` registration is therefore a fallback/integration-test route, not the normal onboarding path.

### Required deployed checks

- Confirm endpoint and schema on the installed RomM version.
- Decide which non-sensitive stable identifiers the Beelink should report.
- Confirm registration does not accidentally adopt another logical installation.
- Confirm disabling or deleting a device and token-revocation behavior in a disposable test account.

## Negotiation

The 5.4 reference documents `POST /api/sync/negotiate` with local save observations including:

- `rom_id`
- `file_name`
- `slot`, normally `autosave`
- optional emulator identity
- `content_hash`
- `updated_at`
- `file_size_bytes`

The documented wire hash is MD5. The project may use SHA-256 internally for local integrity, but it must calculate the protocol-required digest separately.

Saves pair on `(rom_id, slot)`. A `null` slot is archival/manual and is not paired. Negotiation opens a session, and operations contain no local paths; local path selection remains entirely the client's responsibility.

The inspected schema confirms that `saves` is the only required negotiation field. Each local save must include `rom_id`, `file_name`, `updated_at` and `file_size_bytes`; slot, emulator and content hash are optional in the schema but mandatory under this project's safety policy for enabled adapters. Optional `rom_ids` can restrict downloads to installed ROMs without making omitted ROMs deletion candidates, and optional `emulators` can prevent pairing incompatible emulator formats.

The OpenAPI contract requires `roms.read` for `GET /api/roms`, `GET /api/roms/{id}` and `GET /api/roms/by-hash`, and `platforms.read` for `GET /api/platforms`. These routes provide the read-only identity bridge from an approved local ROM hash/platform to RomM's internal `rom_id`; filename-only matching is insufficient.

The response returns a session ID, operation counts and ordered operations with action, ROM ID, filename, reason, optional server save ID, slot, emulator, server timestamp and server content hash. Supported actions are exactly `upload`, `download`, `conflict`, `no_op` and `delete`.

RomM 5.4 intentionally retains multiple timestamp-tagged versions within a non-null slot. `GET /api/saves/summary?rom_id=...` returns each slot's count and explicit `latest` record. The tagged 5.4.0 negotiation source groups server saves by `(rom_id, slot)` and selects the greatest `updated_at` value as the slot head; superseded rows do not participate in negotiation. The same source falls back to comparing client and server timestamps when a device has no sync history. That fallback is useful server behavior but is weaker than this project's safety policy: if the local and remote hashes differ and no common baseline is proven, the planner must report a conflict regardless of the server-proposed direction.

The deployed summary endpoint was then verified against the hash-identified DS ROM. It returned one `autosave` summary containing two versions and an explicit latest record. The latest record is a present 8,192-byte `.sav`, is not missing from server storage, has origin-device metadata, reports emulator `melondsds`, and has a content hash different from the local `.srm`. The local filesystem mtime is later than the server head, but the newly paired device has no common sync baseline; this is therefore a first-sync conflict under project policy, not an automatic upload.

The same read-only identity and summary flow was verified for the priority GameCube candidate. The sole local GCI maps by its six-byte header identity to the sole local RVZ; CRC, MD5 and SHA-1 of that RVZ independently resolve to the same single RomM `ngc` record. Direct save listing and the summary agree on three `autosave` versions and identify the latest row. Two historical versions are `.zip` artifacts from an unused test client; the current head is a present `.gci` carrying origin-device and emulator metadata. The three rows have distinct hashes, the latest GCI differs from local, and the newly paired device has no sync-baseline entry. This is therefore a first-sync conflict under project policy. The Dolphin GCI adapter excludes the superseded ZIP artifacts as incompatible history, but it must block if an incompatible format is ever the explicit latest head rather than silently restoring an older compatible row.

The GameCube local commit sequence was also exercised without network mutation or live-save writes. In disposable runtime storage, the valid GCI produced a byte-identical verified backup, an incoming copy atomically replaced the working path using its original inode, the committed file retained a valid header/block-count-derived length, the source hash/size/mtime remained unchanged, and cleanup removed all temporary files. This proves the single-file local mechanism only; it does not authorize resolving the observed conflict or claim multi-GCI grouping/network execution is complete.

The user reports that every console in the deployed setup uses the default `autosave` slot and that no custom slots/channels have been configured. This resolves the current deployment convention but does not permit the client to hard-code it without checking each RomM response; an unknown, null or mismatched slot must block automatic execution.

### Safety interpretation

| Server action | v1 client interpretation before validation |
|---|---|
| `no_op` | Record a verified match only after hashes and identity are validated |
| `upload` | Proposed action; executor must revalidate local group and remote baseline |
| `download` | Proposed action; stage content, validate it, back up and atomically replace only while inactive |
| `conflict` | Preserve both; require explicit user resolution |
| `delete` | Do not delete locally in v1; preserve the local save and surface a blocked/manual-resolution state |

The server-designated slot head is the authoritative current **remote version**, but it is not automatically the safe winner over a divergent local save.

## Moving bytes and conflict guards

The documented upload endpoint supports `overwrite=false`; with a device identity it returns `409` if the target slot has moved since that device last synchronized it. The documented direct `PUT /api/saves/{id}` update does not provide the same conflict guard and must not be used unless ownership and baseline rules are proven safe.

The inspected OpenAPI route confirms multipart `POST /api/saves` with required `rom_id`, optional device/hash/session metadata, and `overwrite=false` plus `autocleanup=false` defaults. However, its response table declares only `200` and `422`; it does not declare the narrative documentation's guarded `409`. The executor must handle `409` defensively, and M0/M2 must verify it with disposable data before uploads are enabled. Direct `PUT /api/saves/{id}` remains prohibited for normal synchronization.

The documented download endpoint defaults to `optimistic=true`, which marks the device synchronized when content is served. The proposed client policy is:

1. Request downloads with `optimistic=false` where supported.
2. Stage bytes outside the live save path.
3. Validate response, size and both wire/local hashes.
4. Revalidate remote revision/baseline and local execution preconditions.
5. Back up and atomically commit the local file group.
6. Confirm the download only after the committed bytes are verified.

The exact deployed behavior and error schemas remain unverified.

## Version and slot compatibility

The deployed RomM 5.4 protocol uses per-device negotiation and the `autosave` slot convention. The RomM adapter must be selected from the live server version and verified endpoint capabilities, not a single permanently hard-coded schema. RomM 5.4 is the only compatibility band in scope for initial validation; any changed or future server version remains read-only and blocks mutating sync until its protocol is explicitly validated.

## DS interoperability

The user reports a successful two-way load after renaming a specific `.sav` and RetroArch melonDS DS `.srm`. This is evidence for that save/configuration only. The Beelink client boundary ends at RomM; other devices and clients are outside normal project scope.

The required DS bridge is:

1. For upload, copy the inactive local `.srm` to temporary staging with the expected `.sav` name and prove the bytes are identical.
2. Negotiate and upload the staged `.sav` against the mapped RomM ROM and `autosave` slot using the guarded remote baseline.
3. For download, stage the RomM `.sav`, validate identity, size and hashes, then revalidate both the RomM baseline and the unchanged local `.srm` hash.
4. With the emulator still confirmed inactive, back up and atomically replace the live bytes while retaining the local `.srm` name.
5. Preserve both versions on an independent-writer conflict; timestamps alone never choose a winner.

M0 has now recorded the uniquely hash-mapped ROM identity, RomM-visible USA region/revision 1 metadata, `melonDS DS` 1.4.0, absence of a same-stem companion for the observed candidate, and the complete read-only RomM slot/head metadata. The manual `.sav` artifact and current `.srm` are both 8,192 bytes but are not currently identical. The artifact hash matches exactly one retained remote version—the explicit current `Latest` `autosave`—while the live `.srm` matches neither version. This proves RomM retained the artifact bytes unchanged and establishes a safe read-only lineage, but it does not authorize selecting the remote copy over the divergent local revision. A disposable local proof copied the live `.srm` to staged `.sav` and the remote-derived artifact `.sav` to staged `.srm`; both outputs were byte-identical to their sources, both sources remained unchanged, and the temporary files were removed.

The extension bridge is therefore a byte-preserving staging rename/copy, not a content transformation. Emulator-load regression and guarded network transfer behavior remain later disposable integration tests; M0 does not modify a live save or open a mutating sync session.

No live DS save will be changed during M0. Any confirmation must use approved copies in disposable locations.

## Target platform slug mapping

The live heartbeat's filesystem platform list includes every v1 target. The initial server-side mapping is:

| Target platform | Live RomM slug |
|---|---|
| Nintendo DS | `nds` |
| Game Boy Advance | `gba` |
| Nintendo GameCube | `ngc` |
| Nintendo Entertainment System | `nes` |
| Super Nintendo Entertainment System | `snes` |
| PlayStation | `psx` |
| Nintendo 64 | `n64` |
| PlayStation Portable | `psp` |
| Wii | `wii` |
| Nintendo 3DS | `3ds` |
| PlayStation 2 | `ps2` |
| Nintendo Switch | `switch` |

The heartbeat also listed non-target platforms, including a distinct `nintendo-dsi` slug. Adapters must use the RomM ROM/platform identity returned by the API rather than collapsing related systems or guessing from file extensions.

## Deployed-server verification checklist

| Check | State |
|---|---|
| Installed RomM version/build | RomM 5.4.0, verified from UI |
| `GET /api/heartbeat` version/capabilities | Live response verified version 5.4.0, title-ID extraction enabled and device installation enabled; no explicit sync/snapshot capability field was present |
| Client API Token management UI | Present, verified from supplied UI screenshot |
| Devices management UI | Present, verified from supplied UI screenshot |
| Device authorization/pairing flow | Verified against deployed server; selected for v1 |
| Minimal token scopes | Exact six-scope grant verified: `roms.read`, `platforms.read`, `assets.read`, `assets.write`, `devices.read`, `devices.write` |
| Device registration schema/idempotency | Pairing verified to return a device ID/token and the device is visible through `GET /api/devices`; direct-registration idempotency remains untested and is only a fallback |
| Negotiate request/response schema | OpenAPI schema verified; authenticated runtime response pending |
| MD5 wire hash requirement | Documented for the deployed 5.4 version; live behavior unverified |
| `autosave` behavior | Authenticated read-only listing verified 43 version records across eight represented platforms; the mapped DS ROM has two retained `.sav` versions, while the mapped GameCube ROM has three versions (two superseded test-client ZIPs and one latest GCI); both summaries expose explicit heads |
| Snapshot/channel capability | No dedicated capability field appeared in the supplied 5.4.0 heartbeat; slot-based API still requires endpoint verification |
| `optimistic=false` download confirmation | Query parameter and explicit confirmation schema verified; runtime behavior unverified on target |
| Upload `409` conflict guard | Narrative documentation describes it; OpenAPI omits it from declared responses; disposable runtime test required |
| Session completion and retry behavior | Request/response schema verified; failure and retry behavior unknown |
| Reverse proxy | None, user-confirmed |
| Container deployment | Synology Container Manager; `rommapp/romm:latest` and auto-restart verified from supplied UI screenshot; immutable image digest unknown |
| Version-drift policy | Mutable tag observed; re-detect live version/capabilities and fail closed after change |
| Rate limits | Unknown |
| DS remote contract | ROM identity, `.sav`, `autosave`, version history, explicit latest head, content hash, origin metadata, stored-file presence and `melondsds` emulator metadata are verified read-only; local byte compatibility and disposable transfer behavior remain pending |

## Exit condition for this report

This report is complete only when the deployed version and capabilities are recorded, sanitized request/response schemas are captured from a disposable test account, `autosave` semantics are confirmed, and both DS directions are demonstrated against RomM without exposing credentials or modifying a live save.
