---
name: commands
description: Exact verification commands for RommSaveSyncer (Windows dev PC), timings, and how to check the desktop app launches
metadata:
  type: project
---

- Full check: `.\gradlew.bat check --console=plain` (PowerShell). Warm ~30 s; sigil MSVC JNI build ~20 s after `clean`.
- Confirm tests really ran: `find . -path ./third_party -prune -o -name "TEST-*.xml" -print` and compare timestamps (UTC). M1 tests: core/model AppInfoTest (jvmTest), core/saves SigilJniSmokeTest, desktop/app HelloScreenTest.
- `:core:saves:buildSigilHostJni` is UP-TO-DATE on 2nd run; rebuilds fine after clean (VS 18 2026 generator via vswhere).
- `:android:app:assembleDebug`, `:desktop:app:createReleaseDistributable`, `:desktop:app:packageReleaseMsi` all work locally. MSI works WITHOUT WiX installed: Compose plugin downloads wix311 into root `build/wix311`.
- Launch check: jpackage `RommSaveSyncer.exe` spawns a CHILD RommSaveSyncer.exe that owns the window; parent has ~11 MB WS and empty title (not a failure). Use Win32_Process ParentProcessId to find it; kill with `Get-Process RommSaveSyncer | Stop-Process -Force`.
- Only Gradle deprecation: Configuration.setVisible (from a plugin), harmless for now.
- Never run `git submodule update` while the sigil pin is uncommitted.
