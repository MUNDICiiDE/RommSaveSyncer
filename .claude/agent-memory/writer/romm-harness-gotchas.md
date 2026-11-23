---
name: romm-harness-gotchas
description: M2 findings about RomM 5.4.1 API/bootstrapping, Testcontainers harness and the integration-test Gradle plugin
metadata:
  type: project
---

- RomM 5.4.1 CSRF middleware blocks even the first unauthenticated `POST /api/users` (403) unless the `romm_csrftoken` cookie from an earlier response is echoed in `x-csrftoken`. Bearer requests (no session) are exempt. Scan = `POST /api/tasks/scan` (body `{"apis": []}`), status via `GET /api/tasks/{id}`; `/api/roms` is paged (`items`).
- Default library layout `/romm/library/roms/<slug>/<file>`; image declares `VOLUME /romm`, `docker cp`/Testcontainers copy into it works.
- Convention plugin `rommsavesyncer.integration-test` creates the `integrationTest` source set/task (JUnit Jupiter 6.1, not in `check`). Modules just apply it.
- detekt rejects `catch (e: RuntimeException)` (TooGenericExceptionCaught) and bare numbers like 200; use try/finally with a flag and named consts.
- Git Bash: `python -` hangs on the Windows Store shim; never use python/perl in-place edits, use the Edit tool. Set `MSYS_NO_PATHCONV=1` for `docker exec/cp` with absolute container paths.
- Never leave `docker compose up` running; `down -v` and delete `testing/romm-harness/manual` after manual checks.
