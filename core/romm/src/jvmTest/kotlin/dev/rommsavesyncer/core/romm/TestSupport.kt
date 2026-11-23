package dev.rommsavesyncer.core.romm

import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.MockRequestHandleScope
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.toByteArray
import io.ktor.client.request.HttpRequestData
import io.ktor.client.request.HttpResponseData
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

internal const val BASE_URL = "https://romm.test"
internal const val TOKEN = "device-token"

internal typealias Handler = suspend MockRequestHandleScope.(HttpRequestData) -> HttpResponseData

/** Runs a suspending test body; its return type is Unit, which JUnit requires of a test method. */
internal fun runSuspending(block: suspend CoroutineScope.() -> Unit) = runBlocking(block = block)

/** A token source that records what RomM rejected. */
internal class RecordingTokens(
    var token: String? = TOKEN,
) : RommTokenSource {
    val rejected = mutableListOf<String?>()

    override suspend fun accessToken(): String? = token

    override suspend fun onUnauthorized(rejectedToken: String?) {
        rejected += rejectedToken
    }
}

/** A client over a [MockEngine] that answers with [handler] and remembers every request. */
internal class MockRomm(
    val tokens: RecordingTokens = RecordingTokens(),
    handler: Handler,
) {
    private val engine = MockEngine(handler)
    val http = createRommHttpClient(engine)
    val client = KtorRommClient(http, BASE_URL, tokens)
    val deviceAuth = KtorDeviceAuthApi(http, BASE_URL)
    val requests: List<HttpRequestData> get() = engine.requestHistory
    val request: HttpRequestData get() = requests.single()
}

internal fun MockRequestHandleScope.json(
    body: String,
    status: HttpStatusCode = HttpStatusCode.OK,
): HttpResponseData =
    respond(
        content = body,
        status = status,
        headers = headersOf(HttpHeaders.ContentType, "application/json"),
    )

internal fun MockRequestHandleScope.failure(
    status: HttpStatusCode,
    detail: String,
): HttpResponseData = json("""{"detail":"$detail"}""", status)

internal suspend fun HttpRequestData.bodyText(): String = body.toByteArray().decodeToString()

internal suspend fun HttpRequestData.bodyJson(): JsonObject = Json.parseToJsonElement(bodyText()).jsonObject

internal fun HttpRequestData.query(name: String): String? = url.parameters[name]

internal fun JsonElement.field(name: String): JsonElement? = jsonObject[name]

internal val SAVE_JSON =
    """
    {"id":7,"rom_id":3,"user_id":1,"file_name":"Game [2026-11-23 10-00-00-000000].srm","file_size_bytes":4,
     "emulator":"retroarch","slot":"autosave","content_hash":"abc123","origin_device_id":"dev-1",
     "created_at":"2026-11-23T10:00:00+00:00","updated_at":"2026-11-23T10:00:00.5+00:00",
     "device_syncs":[{"device_id":"dev-1","device_name":"PC","last_synced_at":"2026-11-23T10:00:00+00:00",
     "is_untracked":false,"is_current":true}],"screenshot":null}
    """.trimIndent()
