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
- M3: Writer guessed RomM response values in MockEngine fixtures (session status "completed"; RomM 5.4.1 sends "COMPLETED") and assumed RomM conflict semantics without reading comparison.py (negotiate only uses syncs on the NEWEST slot row, so a new version by another device yields UPLOAD by timestamp, not CONFLICT). Cross-check mock JSON and behaviour assumptions against RomM source.
- M3 fix1: Writer added an IT built on a guessed round-trip (upload response `updated_at` == what negotiate compares). False on 5.4.1: add_save returns the merged object (expire_on_commit=False) with microseconds, MariaDB TIMESTAMP stores whole seconds -> deterministic UPLOAD. Writer flags a "risk" but never runs ITs; always run them.
- RomM harness DB is MariaDB 11.8.9: TIMESTAMP(0) TRUNCATES fractions (MySQL would round), so "stored = response truncated to seconds" is deterministic.
- An early assertion failure in a long end-to-end IT hides everything after it; re-check the tail assertions (play_sessions) once fixed.
