# RomM test harness

A throwaway [RomM](https://github.com/rommapp/romm) server for integration tests: `rommapp/romm:5.4.1` on
`mariadb:11.8.9`, with a synthetic ROM library for the v1 systems (GB, GBC, GBA, NES, SNES, N64, DS, PS1, GameCube).
No metadata provider is configured or contacted, and nothing persists between runs.

## In tests (Testcontainers)

Needs a running Docker daemon.

```sh
./gradlew integrationTest -Promm=docker   # all modules' integration tests; not part of `check`
```

A module opts in with `id("rommsavesyncer.integration-test")` and `integrationTestImplementation(project(":testing:romm-harness"))`.
(A Kotlin Multiplatform module, such as `core:romm`, applies `rommsavesyncer.kmp-library` first; its tests are a `jvm`
compilation in `src/jvmIntegrationTest` and the dependency is `"jvmIntegrationTestImplementation"(...)`.) Then:

```kotlin
@RommServer
class MyIntegrationTest {
    @Test
    fun something(env: RommEnvironment) {
        env.baseUrl                 // http://localhost:<mapped port>
        env.admin.get("/api/roms")  // admin client (bearer token; re-logs in on 401)
        env.romId(FixtureRoms.all.first())
    }
}
```

### Approving a device (the device flow, headlessly)

A client starts pairing with `POST /api/auth/device/init`; a person normally approves the code on RomM's
`/pair/device` page. The admin client does the same through the API:

```kotlin
env.admin.approveDevice(userCode, scopes = listOf("assets.read", ...), deviceName = "Test PC")  // returns the device id
env.admin.denyDevice(userCode)                  // the device's next poll gets access_denied
env.admin.revokeDeviceTokens(deviceId)          // later requests with the device's token get 401
```

RomM creates the device and a token bound to it on approval; the device's next poll of
`POST /api/auth/device/token` receives it. RomM allows 10 `init` calls per minute per address.

The first `RommEnvironment` request starts the stack, creates the admin user (`admin` / `admin-password`), scans the
library and waits until every fixture ROM is listed with its hashes. The stack is shared by all test classes in the
JVM and removed (containers, volumes, network) when the run ends. A test that needs its own server uses
`RommStack.start()` and `RommBootstrap.bootstrap(stack.baseUrl)` (see `FreshStartIntegrationTest`).

## By hand, on the LAN (device testing)

```sh
./gradlew :testing:romm-harness:writeFixtureLibrary          # writes testing/romm-harness/manual/library
docker compose -f testing/romm-harness/docker-compose.yml up -d
# RomM is at http://<this PC's LAN address>:8080; create the admin in the setup wizard, then run a library scan.
docker compose -f testing/romm-harness/docker-compose.yml down -v   # removes everything, including the database
```

Open port 8080 in the PC's firewall for other devices. The compose file has no named volumes, so `down -v` leaves
nothing behind.

## Facts checked against RomM 5.4.1

- Library layout: `/romm/library/roms/<platform-slug>/<file>` is the default (`DEFAULT_ROM_STRUCTURE`,
  `backend/config/config_manager.py`); the slugs are `gb gbc gba nes snes n64 nds psx ngc` (`backend/utils/platform_slugs.py`).
- The first user is created with `POST /api/users` and no credentials while no admin exists
  (`backend/endpoints/user.py`). RomM's CSRF middleware still applies: echo the `romm_csrftoken` cookie from any earlier
  response in an `x-csrftoken` header.
- The admin logs in with the OAuth password grant, `POST /api/token` (form fields `grant_type`, `username`, `password`,
  `scope`); bearer requests are exempt from CSRF.
- A scan is `POST /api/tasks/scan` (scope `tasks.run`; body `{"apis": []}` uses no metadata provider), followed on
  `GET /api/tasks/{task_id}`. `GET /api/roms` lists the ROMs (`items[].id`, `md5_hash`, `sha1_hash`, `crc_hash`).
- Approving a device is `POST /api/auth/device/approve` (scope `me.write`; body `user_code`, `approved_scopes`,
  optional `device_name` and `expires_in` of `30d`, `90d` or `1y`), and denying is `POST /api/auth/device/deny`
  (`backend/endpoints/device/auth.py`). Approved scopes must be a subset of what the device requested and the user holds.
- Tokens are listed and revoked with `GET /api/client-tokens` and `DELETE /api/client-tokens/{id}` (scope `me.write`).
