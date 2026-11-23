---
name: commands
description: Exact verification commands for RommSaveSyncer (Windows dev PC), timings, Docker integration-test and manual compose checks, desktop launch check
metadata:
  type: project
---

- Full check: `.\gradlew.bat check --console=plain` (PowerShell). Warm ~30 s; ~50 s when sigil JNI rebuilds / new module.
- Confirm check excludes integration tests: `.\gradlew.bat check --dry-run` and grep for "integrationTest" (only ktlint over that source set appears; compileIntegrationTestKotlin is NOT in check).
- Confirm tests really ran: TEST-*.xml under `*/build/test-results/<task>/`, compare timestamps (UTC). KMP modules: unit = `jvmTest`, Docker = `integrationTest` (sources `src/jvmIntegrationTest`).
- Integration (M2+): `.\gradlew.bat integrationTest --console=plain --continue` ~3 min (harness + core:romm run in parallel, 2 stacks). core:romm DeviceAuthIntegrationTest ~100 s (5 s poll interval per pairing). Run with run_in_background + log to scratchpad; wait with `until grep -q "BUILD \(SUCCESSFUL\|FAILED\)" log; do sleep 5; done`.
- RomM 5.4.1 source for DTO/behaviour checks: `docker create --name x rommapp/romm:5.4.1; MSYS_NO_PATHCONV=1 docker cp x:/backend <scratch>; docker rm -v x` (-v! image VOLUMEs leave 2 anon volumes otherwise). Key files: endpoints/saves.py, endpoints/sync/__init__.py, handler/sync/comparison.py, endpoints/device/auth.py, utils/device_auth.py.
- Failure detail: grep `AssertionFailedError` in core/romm/build/test-results/integrationTest/*.xml. Full IT run ~3m20s; check warm ~15 s.
- Docker cleanup: containers/anon volumes/network gone right after; Ryuk container lingers up to ~60 s then exits by itself.
- Manual compose: `:testing:romm-harness:writeFixtureLibrary` -> `docker compose -f testing/romm-harness/docker-compose.yml up -d` (~65 s to heartbeat on :8080) -> `down -v` -> delete testing/romm-harness/manual.
- `:core:saves:buildSigilHostJni` UP-TO-DATE on 2nd run; rebuilds after clean (VS 18 2026 via vswhere).
- `:android:app:assembleDebug`, `:desktop:app:createReleaseDistributable`, `:desktop:app:packageReleaseMsi` work locally; MSI auto-downloads wix311 into root `build/wix311`.
- `check` compiles a KMP android target only if android:app depends on the module (core:romm not yet); verify with `:core:romm:compileAndroidMain` (~5 s).
- Launch check: jpackage exe spawns a CHILD RommSaveSyncer.exe owning the window; find via Win32_Process ParentProcessId; kill with `Get-Process RommSaveSyncer | Stop-Process -Force`.
- 2nd IT run: add `--rerun` (`integrationTest --rerun --continue`) or Gradle skips it as UP-TO-DATE. M3 final: 27 IT testcases (core:romm 20, harness 7).
- Never run `git submodule update` while the sigil pin is uncommitted.
