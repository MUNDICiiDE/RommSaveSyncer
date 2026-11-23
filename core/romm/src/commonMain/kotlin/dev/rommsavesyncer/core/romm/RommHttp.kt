package dev.rommsavesyncer.core.romm

import io.ktor.client.HttpClient
import io.ktor.client.engine.HttpClientEngine
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.serialization.kotlinx.json.json
import kotlinx.serialization.json.Json

private const val CONNECT_TIMEOUT_MILLIS = 15_000L
private const val REQUEST_TIMEOUT_MILLIS = 120_000L
private const val SOCKET_TIMEOUT_MILLIS = 60_000L

/** The JSON settings for every RomM payload: unknown fields are skipped, nulls are left out. */
internal val RommJson: Json =
    Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

/** The engine each platform uses (OkHttp); see the `jvmMain` and `androidMain` actuals. */
expect fun defaultRommEngine(): HttpClientEngine

/**
 * An HTTP client set up for RomM: JSON bodies and timeouts. HTTP error statuses are not turned into exceptions
 * here; [RommTransport] maps them to [RommException]. Pass `MockEngine` in tests.
 */
fun createRommHttpClient(engine: HttpClientEngine = defaultRommEngine()): HttpClient =
    HttpClient(engine) {
        expectSuccess = false
        install(ContentNegotiation) { json(RommJson) }
        install(HttpTimeout) {
            connectTimeoutMillis = CONNECT_TIMEOUT_MILLIS
            requestTimeoutMillis = REQUEST_TIMEOUT_MILLIS
            socketTimeoutMillis = SOCKET_TIMEOUT_MILLIS
        }
    }
