package dev.rommsavesyncer.testing.harness

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.jsonPrimitive
import java.net.URI
import java.net.URLEncoder
import java.net.http.HttpClient
import java.net.http.HttpRequest
import java.net.http.HttpResponse
import java.time.Duration

/**
 * An HTTP client that is logged in to RomM as the admin user, with a bearer token from the OAuth password grant
 * (`POST /api/token`, RomM 5.4.1 `backend/endpoints/auth.py`). Bearer requests are exempt from RomM's CSRF check.
 *
 * Paths are relative to the server, such as `/api/roms`. If a request is rejected with 401 (the token expired),
 * the client logs in again and repeats it once.
 */
class RommAdminClient(
    private val baseUrl: String,
    private val username: String = ADMIN_USERNAME,
    private val password: String = ADMIN_PASSWORD,
) {
    private val http = HttpClient.newBuilder().connectTimeout(REQUEST_TIMEOUT).build()

    @Volatile
    private var accessToken: String? = null

    fun get(path: String): RommResponse = request("GET", path)

    fun postJson(
        path: String,
        body: JsonElement? = null,
    ): RommResponse = request("POST", path, body?.toString())

    fun request(
        method: String,
        path: String,
        jsonBody: String? = null,
    ): RommResponse {
        val first = send(method, path, jsonBody, accessToken ?: login())
        return if (first.status == HTTP_UNAUTHORIZED) send(method, path, jsonBody, login()) else first
    }

    private fun send(
        method: String,
        path: String,
        jsonBody: String?,
        token: String,
    ): RommResponse {
        val publisher = jsonBody?.let { HttpRequest.BodyPublishers.ofString(it) } ?: HttpRequest.BodyPublishers.noBody()
        val builder =
            HttpRequest
                .newBuilder(URI.create(baseUrl + path))
                .timeout(REQUEST_TIMEOUT)
                .header("Authorization", "Bearer $token")
                .method(method, publisher)
        if (jsonBody != null) builder.header("Content-Type", "application/json")
        val response = http.send(builder.build(), HttpResponse.BodyHandlers.ofString())
        return RommResponse(response.statusCode(), response.body())
    }

    private fun login(): String {
        val form =
            mapOf(
                "grant_type" to "password",
                "username" to username,
                "password" to password,
                "scope" to ADMIN_SCOPES.joinToString(" "),
            ).entries.joinToString("&") { (key, value) -> "$key=${URLEncoder.encode(value, Charsets.UTF_8)}" }
        val request =
            HttpRequest
                .newBuilder(URI.create("$baseUrl/api/token"))
                .timeout(REQUEST_TIMEOUT)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .POST(HttpRequest.BodyPublishers.ofString(form))
                .build()
        val response = http.send(request, HttpResponse.BodyHandlers.ofString())
        val result = RommResponse(response.statusCode(), response.body()).requireSuccess("Admin login")
        return result
            .jsonObject()
            .getValue("access_token")
            .jsonPrimitive.content
            .also { accessToken = it }
    }

    companion object {
        const val ADMIN_USERNAME = "admin"
        const val ADMIN_PASSWORD = "admin-password"
        const val ADMIN_EMAIL = "admin@example.com"

        private const val HTTP_UNAUTHORIZED = 401
        private val REQUEST_TIMEOUT = Duration.ofSeconds(30)

        /** Every scope RomM 5.4.1 defines (`handler/auth/constants.py`), which an admin may request. */
        private val ADMIN_SCOPES =
            listOf(
                "me.read",
                "me.write",
                "roms.read",
                "roms.write",
                "roms.user.read",
                "roms.user.write",
                "platforms.read",
                "platforms.write",
                "assets.read",
                "assets.write",
                "devices.read",
                "devices.write",
                "firmware.read",
                "firmware.write",
                "collections.read",
                "collections.write",
                "playlists.read",
                "playlists.write",
                "users.read",
                "users.write",
                "tasks.run",
                "logs.read",
            )
    }
}
