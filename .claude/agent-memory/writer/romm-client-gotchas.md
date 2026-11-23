---
name: romm-client-gotchas
description: M3 findings on RomM 5.4.1 client API facts, KMP/Ktor setup and detekt/ktlint traps in core:romm
metadata:
  type: project
---

- RomM 5.4.1 facts that differ from docs/plan wording: save upload metadata (rom_id, slot, device_id, session_id, content_hash, overwrite, emulator, autocleanup) are QUERY params, only `saveFile`/`screenshotFile` are multipart; device flow init path is `/api/auth/device/init` (not /authorize); poll errors are HTTP 400 with `detail` = authorization_pending|slow_down|expired_token|access_denied; 401 = no/invalid token, 403 = missing scope.
- RomM timestamps are whole seconds (MariaDB TIMESTAMP): integration tests that depend on a later write being newer need a >1s wait. Device-flow `init` is rate limited to 10/min per IP, so pair devices once per JVM (`TwoDevices.of(env)`).
- Ktor: `MockRomm(tokens) { handler }` style needs the lambda param LAST (trailing lambda binds to last param). Test method bodies use `runSuspending {}`/`runBlocking<Unit>` (JUnit needs void). JUnit5 `Assertions.assertNotNull` returns void in Kotlin: use `checkNotNull`.
- KMP integration tests: plugin `rommsavesyncer.integration-test` after `kmp-library` makes a jvm compilation `integrationTest` (sources `src/jvmIntegrationTest`, config prefix `jvmIntegrationTest`, jvmTarget 21, unit task is `jvmTest`). Task names: `compileIntegrationTestKotlinJvm`, `compileTestKotlinJvm`, `compileAndroidMain`.
- Serialization plugin cannot be applied by id from a module script (build-logic deps are not visible): use `rommsavesyncer.serialization`.
- detekt: TooManyFunctions counts >=11 per class/file and interface (split APIs into small interfaces + delegation); ThrowsCount max 2; MaxLineLength 120 also hits raw strings; MagicNumber applies to tests too.
- ktlint: run `ktlintFormat` after writing; function with 2+ params is wrapped one per line.
