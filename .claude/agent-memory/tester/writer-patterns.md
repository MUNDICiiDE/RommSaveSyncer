---
name: writer-patterns
description: Recurring Writer defect patterns and fragile areas in RommSaveSyncer worth regression checks
metadata:
  type: project
---

- Writer does not run tests itself (classifier blocks it; M2 it never ran integrationTest), so always run the full suite incl. integrationTest when Docker code changes.
- Writer's environment claims can be wrong (M1: said MSI can't build without WiX; it can). Verify such claims.
- Writer leaves plan/docs text stale or adds no-op flags (M2: `-Promm=docker` documented but unused).
- Fragile: BuildSigilHostJni (wipes CMake dir per execution, MSVC unistd.h shim, isWindows() branching). Re-check UP-TO-DATE + post-clean rebuild when build-logic changes.
- Fragile: RomM bootstrap (CSRF cookie->x-csrftoken for first user, /api/token scopes, scan polling). Re-run integrationTest when RommBootstrap/RommStack change.
- CI (ci.yml) can't be run locally; Linux gcc JNI build, headless Compose test, and Linux Docker integrationTest unverified until CI runs.
