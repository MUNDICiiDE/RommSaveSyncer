package dev.rommsavesyncer.testing.harness

import dev.rommsavesyncer.testing.fixtures.FixtureRom
import dev.rommsavesyncer.testing.fixtures.FixtureRoms
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import java.net.CookieManager
import java.net.URI
import java.net.http.HttpClient
import java.net.http.HttpRequest
import java.net.http.HttpResponse
import java.time.Duration

/**
 * Takes a fresh RomM server to the state the tests start from: creates the admin user, scans the fixture
 * library with no metadata providers, and waits until every fixture ROM is listed with its hashes.
 */
object RommBootstrap {
    private const val CSRF_COOKIE = "romm_csrftoken"
    private const val CSRF_HEADER = "x-csrftoken"
    private val REQUEST_TIMEOUT = Duration.ofSeconds(30)
    private val SCAN_TIMEOUT = Duration.ofMinutes(3)
    private val POLL_INTERVAL = Duration.ofSeconds(1)
    private val FAILED_TASK_STATUSES = setOf("failed", "canceled", "stopped")
    private val HASH_FIELDS = listOf("md5_hash", "sha1_hash", "crc_hash")
    private val plainClient = HttpClient.newHttpClient()

    fun bootstrap(baseUrl: String): RommEnvironment {
        createAdmin(baseUrl)
        val admin = RommAdminClient(baseUrl)
        val scanTaskId = startScan(admin)
        return RommEnvironment(baseUrl, admin, awaitFixtureRoms(admin, scanTaskId))
    }

    /** An unauthenticated `GET /api/heartbeat`; its `SYSTEM` block has the version and the first-run state. */
    fun heartbeat(baseUrl: String): RommResponse {
        val request = HttpRequest.newBuilder(URI.create("$baseUrl/api/heartbeat")).timeout(REQUEST_TIMEOUT).build()
        val response = plainClient.send(request, HttpResponse.BodyHandlers.ofString())
        return RommResponse(response.statusCode(), response.body())
    }

    /**
     * `POST /api/users` creates the first user without credentials while no admin exists, but RomM's CSRF
     * middleware still wants its double-submit token: the cookie set by any earlier response, echoed in a header.
     */
    private fun createAdmin(baseUrl: String) {
        val cookies = CookieManager()
        val http = HttpClient.newBuilder().cookieHandler(cookies).build()
        val heartbeat = HttpRequest.newBuilder(URI.create("$baseUrl/api/heartbeat")).timeout(REQUEST_TIMEOUT).build()
        http.send(heartbeat, HttpResponse.BodyHandlers.discarding())
        val csrfToken =
            cookies.cookieStore.cookies
                .first { it.name == CSRF_COOKIE }
                .value

        val body =
            buildJsonObject {
                put("username", RommAdminClient.ADMIN_USERNAME)
                put("email", RommAdminClient.ADMIN_EMAIL)
                put("password", RommAdminClient.ADMIN_PASSWORD)
                put("role", "admin")
            }
        val request =
            HttpRequest
                .newBuilder(URI.create("$baseUrl/api/users"))
                .timeout(REQUEST_TIMEOUT)
                .header("Content-Type", "application/json")
                .header(CSRF_HEADER, csrfToken)
                .POST(HttpRequest.BodyPublishers.ofString(body.toString()))
                .build()
        val response = http.send(request, HttpResponse.BodyHandlers.ofString())
        RommResponse(response.statusCode(), response.body()).requireSuccess("Creating the admin user")
    }

    /** Queues a quick scan (`POST /api/tasks/scan`) with `apis` empty, so no metadata provider is contacted. */
    private fun startScan(admin: RommAdminClient): String {
        val body = buildJsonObject { put("apis", JsonArray(emptyList())) }
        val response = admin.postJson("/api/tasks/scan", body).requireSuccess("Starting the library scan")
        return response
            .jsonObject()
            .getValue("task_id")
            .jsonPrimitive.content
    }

    private fun awaitFixtureRoms(
        admin: RommAdminClient,
        scanTaskId: String,
    ): Map<FixtureRom, JsonObject> {
        val deadline = System.nanoTime() + SCAN_TIMEOUT.toNanos()
        while (true) {
            val scanned = hashedFixtureRoms(admin)
            if (scanned.size == FixtureRoms.all.size) return scanned
            check(taskStatus(admin, scanTaskId) !in FAILED_TASK_STATUSES) { "The library scan did not finish" }
            check(System.nanoTime() < deadline) {
                "Only ${scanned.size} of ${FixtureRoms.all.size} fixture ROMs were scanned within $SCAN_TIMEOUT"
            }
            Thread.sleep(POLL_INTERVAL)
        }
    }

    private fun taskStatus(
        admin: RommAdminClient,
        taskId: String,
    ): String? {
        val response = admin.get("/api/tasks/$taskId").requireSuccess("Reading the scan task")
        return response
            .jsonObject()
            .getValue("status")
            .jsonPrimitive.contentOrNull
    }

    /** The fixture ROMs `GET /api/roms` lists with all three hashes filled in. */
    private fun hashedFixtureRoms(admin: RommAdminClient): Map<FixtureRom, JsonObject> {
        val query = "limit=100&with_char_index=false&with_filter_values=false&with_rom_id_index=false"
        val page = admin.get("/api/roms?$query").requireSuccess("Listing ROMs").jsonObject()
        val listed = page.getValue("items").jsonArray.map { it.jsonObject }
        return buildMap {
            for (rom in FixtureRoms.all) {
                val match = listed.firstOrNull { it.text("fs_name") == rom.fileName }
                if (match != null && HASH_FIELDS.all { !match.text(it).isNullOrEmpty() }) put(rom, match)
            }
        }
    }

    private fun JsonObject.text(key: String): String? = (get(key) as? JsonPrimitive)?.contentOrNull
}
