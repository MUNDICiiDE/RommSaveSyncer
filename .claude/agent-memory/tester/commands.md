---
name: commands
description: Exact verification commands for RommSaveSyncer (Windows dev PC), timings, Docker integration-test and manual compose checks, desktop launch check
metadata:
  type: project
---

- Full check: `.\gradlew.bat check --console=plain` (PowerShell). Warm ~30 s; ~55 s when sigil JNI rebuilds.
- Confirm check excludes integration tests: `.\gradlew.bat check --dry-run` and grep for "integrationTest" (only ktlint over that source set appears; compileIntegrationTestKotlin is NOT in check).
- Confirm tests really ran: TEST-*.xml under `*/build/test-results/<task>/`, compare timestamps (UTC).
- Integration (M2+): `.\gradlew.bat integrationTest --console=plain` ~3.5 min (FreshStart ~140 s for 2 stacks, shared stack ~70 s). Run with run_in_background + log to scratchpad. `-Promm=docker` is a no-op property.
- Docker cleanup: containers/anon volumes/network gone right after; Ryuk container lingers up to ~60 s then exits by itself. RomM image declares VOLUME /romm and /redis-data (anon volumes, removed by Testcontainers stop).
- Manual compose: `:testing:romm-harness:writeFixtureLibrary` -> `docker compose -f testing/romm-harness/docker-compose.yml up -d` (~65 s to heartbeat on :8080) -> `down -v` -> delete testing/romm-harness/manual.
- `:core:saves:buildSigilHostJni` UP-TO-DATE on 2nd run; rebuilds after clean (VS 18 2026 via vswhere).
- `:android:app:assembleDebug`, `:desktop:app:createReleaseDistributable`, `:desktop:app:packageReleaseMsi` work locally; MSI auto-downloads wix311 into root `build/wix311`.
- Launch check: jpackage exe spawns a CHILD RommSaveSyncer.exe owning the window; find via Win32_Process ParentProcessId; kill with `Get-Process RommSaveSyncer | Stop-Process -Force`.
- Never run `git submodule update` while the sigil pin is uncommitted.
