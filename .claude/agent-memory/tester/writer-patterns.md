---
name: writer-patterns
description: Recurring Writer defect patterns and fragile areas in RommSaveSyncer worth regression checks
metadata:
  type: project
---

- Writer does not run tests itself (classifier blocks it), so always run the full suite.
- Writer's environment claims can be wrong (M1: said MSI can't build without WiX; it can). Verify such claims rather than trusting them.
- Writer leaves plan text stale after deviations (plan §6 still says `packageReleaseDistributable`; real task is `createReleaseDistributable`).
- Fragile: BuildSigilHostJni (wipes CMake dir per execution, MSVC unistd.h shim via CFLAGS, Windows-only isWindows() branching). Re-check UP-TO-DATE + post-clean rebuild when build-logic changes.
- CI (ci.yml) can't be run locally; Linux gcc JNI build and headless Compose test on Linux are unverified until CI runs.
