# M0 Adapter Risk and Readiness Matrix

**Specification:** 1.2

**Audit date:** 2026-10-14

**Status:** M0 classification approved 2026-10-14; no adapter is enabled for automatic writes

## Status definitions

| Status | Meaning |
|---|---|
| Planned | Represented by the multi-system design, with no target validation yet |
| Discovery-ready | Read-only path and identity discovery is sufficiently understood to prototype |
| Fixture-validated | Both directions and failures pass against disposable fixtures |
| Target-validated | Approved round trips pass on the actual configured emulator/device |
| Unsupported | Known unsafe or incompatible configuration; no automatic writes |

Every concrete platform/emulator/core/profile combination advances independently. A platform remains disabled until its own validation gate passes.

The user reports RomM libraries for all 12 target platforms. The Beelink's live ES-DE settings, custom-system overlay and bundled Unix system definition establish the definition defaults for every target. ES-DE persists the GameCube and Wii system selections as top-level `alternativeEmulator` blocks in their gamelist fragments; both labels are `Dolphin (Standalone)`. These blocks are outside individual `<game>` records and therefore override the bundled RetroArch `dolphin` default at system scope. No per-game alternative was found. Only the priority DS, GBA and standalone GameCube GCI configurations have deeper local evidence; the other targets remain unexercised and disabled until their installation, save root and save model are validated.

The live RomM heartbeat contains a filesystem slug for every target (`nds`, `gba`, `ngc`, `nes`, `snes`, `psx`, `n64`, `psp`, `wii`, `3ds`, `ps2`, `switch`). It also distinguishes `nintendo-dsi` from `nds`; platform mappings must therefore come from RomM identity rather than extension or display-name guesses.

The deployed RomM setup is reported to use the default `autosave` slot for every console with no custom slot/channel choices. Each adapter must nevertheless compare its proposed slot with live RomM negotiation metadata and fail closed on null, unknown or mismatched values.

## Initial matrix

| Priority | Platform | Candidate configuration | Save ownership model | Initial risk | Current status | Primary unknowns |
|---:|---|---|---|---|---|---|
| 1 | Nintendo DS | ES-DE-selected RetroArch melonDS DS 1.4.0 live `.srm` ↔ staged RomM `autosave` `.sav` | Single game file for observed candidate; adapter must still detect companions | Medium | M0 read-only and staging contract verified | Emulator-load regression and guarded live transfer remain later-milestone tests |
| 2 | Game Boy Advance | ES-DE-selected RetroArch mGBA 0.10-dev | Per-game battery save, possible RTC | Low–Medium | Disabled pending fixture | Zero local GBA ROMs/saves; validate extension, RTC grouping and behavior with disposable/homebrew data |
| 3 | Nintendo GameCube | ES-DE system alternative `Dolphin (Standalone)`; Dolphin 2609a, Slot A GCI Folder, Slot B None | Multiple `.gci` files, region and slot aware | High | M0 single-GCI contract verified | Multi-GCI grouping and guarded network transfer remain later fixture tests |
| 4 | Nintendo Entertainment System | ES-DE default RetroArch `mesen` (`Mesen`) | Usually absent or per-game battery save | Low–Medium | Planned/disabled | Verify installed core/version, mapper/save behavior, root and save sorting |
| 5 | Super Nintendo Entertainment System | ES-DE default RetroArch `snes9x` (`Snes9x - Current`) | Per-game battery save, possible RTC | Medium | Planned/disabled | Verify installed core/version, RTC/companion grouping, root and save sorting |
| 6 | PlayStation | ES-DE default RetroArch `mednafen_psx` (`Beetle PSX`) | Per-game or shared memory card | High | Planned/disabled | Verify installed core/version, card mode, serial/multi-disc identity, root and shared-card exclusion |
| 7 | Nintendo 64 | ES-DE default RetroArch `mupen64plus_next` (`Mupen64Plus-Next`) | Core-dependent file set | High | Planned/disabled | Verify installed core/version, root and EEPROM/SRAM/Flash/controller-pak mapping |
| 8 | PlayStation Portable | ES-DE default RetroArch `ppsspp` | Core-specific file/directory model to verify | Medium–High | Planned/disabled | Do not infer standalone PPSSPP's observed root; verify core behavior, game ID and complete ownership |
| 9 | Wii | ES-DE system alternative `Dolphin (Standalone)` | Per-title directory tree | High | Planned/disabled | Verify title ID, complete group, shared metadata and safe restore model in the observed standalone Wii root |
| 10 | Nintendo 3DS | Custom-system default launches through RetroArch (`Citra`; exact core macro unresolved) | Per-title/profile model to verify | Very high | Planned/disabled | Verify exact core/install, active root, title IDs, profiles, encryption/keys boundaries and extdata exclusions |
| 11 | PlayStation 2 | ES-DE default RetroArch `pcee2` (`PCEE2`) | Folder card, per-game card or shared card | Very high | Planned/disabled | Verify installed core/version, actual card mode/root, serial identity and safe isolation; shared images disabled |
| 12 | Nintendo Switch | Custom-system default `Ryujinx (Standalone)` | Per-title/per-user directory tree | Very high | Planned/disabled | Verify installation, active user/profile, title ID, metadata grouping and firmware/version compatibility |

## Cross-adapter validation contract

Each adapter must document and test:

1. Supported platform, emulator/core and minimum verified versions.
2. ROM/game identity rules, including region, revision, serial/title ID and multi-disc behavior.
3. Exact save root and whether redirects, symlinks, Flatpak paths or per-game overrides apply.
4. Complete save ownership: single file, companion-file group, memory card or directory tree.
5. Read validation and corruption/malformed detection.
6. Activity detection and conservative behavior when activity is unknown.
7. Stable-file/group detection.
8. Backup and atomic restoration strategy for the whole group.
9. RomM filename, emulator metadata, slot/channel and grouping representation.
10. Normal, missing, malformed, conflicting, region-mismatched, interrupted and disk-full fixtures.

## Observed candidate roots

Sanitized EmuDeck symlinks confirm these candidate roots on the Beelink:

| Adapter area | Observed candidate root | What remains unknown |
|---|---|---|
| RetroArch systems | `<user>/.var/app/org.libretro.RetroArch/config/retroarch/saves` | Effective directory settings, core/content sorting, selected core and per-game overrides |
| Dolphin GameCube | `<user>/.var/app/org.DolphinEmu.dolphin-emu/data/dolphin-emu/GC` | Path/activity revalidation and a representative multi-GCI grouping fixture |
| Dolphin Wii | `<user>/.var/app/org.DolphinEmu.dolphin-emu/data/dolphin-emu/Wii` | Per-title boundary and complete file tree |
| PPSSPP | `<user>/.var/app/org.ppsspp.PPSSPP/config/ppsspp/PSP/SAVEDATA` | Active memory-stick/profile configuration and complete title directories |
| Azahar | `<user>/Emulation/storage/azahar/sdmc` | Installed version, selected profile/title identity and safe subtrees |
| Ryujinx | `<user>/.config/Ryujinx/bis/user/save` plus adjacent `saveMeta` | Installed version, active user/profile, title mapping and metadata grouping |

State directories were observed separately and remain outside v1 automatic transfer.

The live ES-DE application data is under `<user>/ES-DE`, with settings at `settings/es_settings.xml` and the active custom overlay at `custom_systems/es_systems.xml`. A disposable extraction of `<user>/Applications/ES-DE.AppImage` found seven bundled system-definition candidates and selected `usr/share/es-de/resources/systems/unix/es_systems.xml` as the Unix definition covering the target set. Command inspection maps the definition defaults to RetroArch cores `melondsds`, `mgba`, `dolphin` (GameCube and Wii), `mesen`, `snes9x`, `mupen64plus_next`, `mednafen_psx`, `pcee2` and `ppsspp`; custom `Citra` also invokes RetroArch but its core token was not exposed by this sanitized classifier. Only custom `Ryujinx (Standalone)` is a standalone definition default. Fragment-tolerant structural inspection then found top-level `<alternativeEmulator><label>Dolphin (Standalone)</label></alternativeEmulator>` blocks in both GameCube and Wii gamelist fragments. Adapter discovery must apply these system-level alternatives before considering the definition default and must separately honor any future per-game alternative.

RetroArch's effective global configuration points at the observed save root, disables saving beside content, disables global core/content sorting, and uses a 10-second autosave interval. Both melonDS core binaries are installed, while ES-DE selects `melonDS DS` 1.4.0. A `melonDS DS` subdirectory exists, but the user's relevant live save is at the root. The root `.srm` is the sole candidate; the same-game `.sav` was manually created for earlier sync testing and must be ignored by discovery. Both files are 8,192 bytes but are not currently identical. The excluded `.sav` matches exactly one remote version, specifically RomM's current `Latest` `autosave`, proving its bytes survived storage unchanged; the live `.srm` is a divergent local revision. The candidate has an approved timestamp, MD5 and SHA-256 baseline for later copy-based compatibility tests; no same-stem companion was observed.

Authenticated RomM metadata contains nine Nintendo DS `autosave` records, all using `.sav` with content hashes and origin-device metadata. Five records share the local candidate's 8,192-byte size, but none has the same MD5 as the current local `.srm`. This proves that size cannot select the game and that the current local bytes are not already represented by any returned DS `autosave`; it does not disprove extension-only compatibility or establish which copy is newer. The save stem identifies exactly one local DS ROM but matches two remote DS records, proving that filename matching is also insufficient. A transient local-ROM hash lookup using CRC, MD5 and SHA-1 resolved all three methods to one RomM DS ROM. Both remote records belong to that ROM and `autosave` slot, have distinct content hashes and are shown by RomM as two retained versions with one explicit `Latest` head. The deployed summary endpoint confirms that head is a present 8,192-byte `.sav` with origin metadata and emulator `melondsds`. Its content differs from the local `.srm`; although the local filesystem mtime is later, the new device has no common baseline, so project policy classifies this initial state as a conflict rather than an automatic upload.

Dolphin's GameCube root contains `EUR`, `JAP` and `USA`. The initial audit found zero files, while a later scan found one USA `.gci` and one `.raw`, proving the discovery snapshot changed during M0. The two candidate ROM directories resolve to one target and their two observations identify one unique `.rvz`; planners must deduplicate such aliases. The GCI's game code, maker code and internal name are present and valid, its header region agrees with the `USA` path, and its two-block declaration exactly accounts for its 16,448-byte length. Dolphin's read-only header command returned a six-byte disc identity that exactly matches the GCI game and maker codes, proving that the GCI belongs to the unique local `.rvz`. CRC, MD5 and SHA-1 lookups of that RVZ independently resolve to the same single RomM record, whose platform is `ngc`. The 68-byte `.raw` has no configuration reference, cannot be a raw GameCube memory-card image and is excluded from the save group.

Direct RomM listing for the mapped ROM returns three distinct `autosave` rows: two `.zip` artifacts created by an unused test client and one `.gci`, which is also the explicit latest head. The ZIPs are retained server history but are excluded from this Dolphin GCI adapter's restore candidates. This exclusion is safe only while a supported `.gci` remains the explicit current head; an incompatible latest format must block instead of causing selection of an older row. The remote GCI hash differs from the local GCI, and the newly paired device has zero sync entries, conclusively making this a first-sync conflict under project policy.

A local disposable proof copied the valid GCI into runtime storage, created and hash-verified a byte-identical backup, atomically replaced the working path with the staged file's inode, revalidated the final block-count/length structure, verified the live source hash/size/mtime remained unchanged and removed the temporary directory. This establishes the single-GCI backup/replacement mechanics without modifying a live save; a representative multi-GCI fixture and guarded network transfer remain later tests.

No explicit Dolphin memory-card slot/path override was found in `Dolphin.ini`. The installed UI confirms Slot A uses `GCI Folder` and Slot B is empty/None. Seventy-four per-game settings files contain no memory-card override keys.

## Early adapter gates

### Nintendo DS

- ES-DE selection is confirmed as RetroArch `melonDS DS` 1.4.0; both melonDS core binaries and their matching configuration trees are present.
- Keep the global-root discovery rule from the effective RetroArch settings; do not redirect saves merely because core-specific configuration directories exist.
- Explicitly exclude the manually created `.sav` test artifact from discovery.
- Treat the root `.srm` as mapped to the uniquely hash-identified RomM DS ROM; its metadata/hash baseline has been recorded, and size, save hash and filename are each proven insufficient on their own.
- Treat extension-only staging as verified: disposable `.srm`→`.sav` and `.sav`→`.srm` copies preserved all bytes, left both sources unchanged and cleaned up temporary files.
- No same-stem companion belongs to the observed candidate; discovery must still detect and fail safely on companions for other titles/configurations.
- Treat the read-only RomM contract as verified: uniquely hash-mapped ROM, `autosave`, `.sav`, retained versions, explicit latest head, hash, origin metadata and `melondsds` emulator metadata.
- Pass both directions and conflict preservation in disposable storage.

### Game Boy Advance

- ES-DE selection and installed core metadata are confirmed as RetroArch `mGBA` 0.10-dev; three core override files contain no save-path directives.
- The local GBA ROM directory contains zero recognized ROMs, and the shared RetroArch root contains no save or companion matching a GBA ROM.
- Obtain an approved disposable/homebrew ROM and save fixture before testing battery-save extension or RTC/companion grouping.
- Validate ROM identity and filename mapping.
- Pass both directions and conflict preservation in disposable storage.

### Dolphin GameCube GCI

Dolphin source exposes distinct GCI folder paths for slots A and B, derives region-specific paths, and reads GCI headers to identify current-game files. Therefore the adapter must not infer ownership from filename alone.

References:

- [Dolphin GCI path configuration](https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/Core/Config/MainSettings.cpp)
- [Dolphin GCI directory implementation](https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/Core/HW/GCMemcard/GCMemcardDirectory.cpp)

Gate requirements:

- Record Dolphin version, user directory and configured slot device types.
- Parse the top-level GameCube gamelist `alternativeEmulator` block before the bundled system definition; do not fall back to RetroArch `dolphin` while `Dolphin (Standalone)` is selected.
- Treat the observed differing local and latest remote GCI with no device baseline as a first-sync conflict; never choose by timestamp.
- Exclude the two superseded foreign-client ZIP artifacts from direct GCI restoration while retaining them as visible history; block if an unsupported format is ever the remote head.
- Treat single-GCI disposable backup/atomic replacement as verified; add a representative multi-GCI fixture before enabling group writes.
- Resolve custom/default paths for both slots and all active regions.
- Revalidate the GCI header game code, maker code, region, block count and expected length before planning and again before execution.
- Determine whether one title can own multiple GCI files and what constitutes a complete group.
- Preserve slot and region without touching unrelated GCI files.
- Back up and restore the coherent group in a disposable copy.

## Explicitly disabled configurations

Until separately proven safe, automatic writes remain disabled for:

- Shared raw memory-card images.
- Unpartitioned shared directories whose files cannot be attributed to one game.
- Save states.
- Unknown or ambiguous game identity.
- Missing/unmounted roots.
- Active games or unknown activity state.
- Cross-core or cross-emulator formats without compatibility evidence.
- Any configuration that cannot create and verify a recoverable backup.

## Exit condition for this report

This M0 matrix was approved on 2026-10-14. The emulator command and alternative-emulator inventory is complete. Unconfigured or unexercised save behavior is explicitly disabled rather than inferred; those rows advance only through their individual adapter gates. DS, GBA and Dolphin GameCube GCI retain the approved early-validation order, with GBA awaiting a legal fixture and Dolphin multi-GCI grouping awaiting a representative fixture. No row is approved for automatic writes.
