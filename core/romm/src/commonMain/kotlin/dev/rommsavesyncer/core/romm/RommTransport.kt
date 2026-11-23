package dev.rommsavesyncer.core.romm

import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.HttpRequestBuilder
import io.ktor.client.request.bearerAuth
import io.ktor.client.request.request
import io.ktor.client.request.setBody
import io.ktor.client.statement.HttpResponse
import io.ktor.client.statement.bodyAsBytes
import io.ktor.client.statement.bodyAsText
import io.ktor.http.ContentType
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import io.ktor.http.contentType
import io.ktor.http.isSuccess
import io.ktor.serialization.ContentConvertException
import kotlinx.io.IOException
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

private const val MAX_DETAIL_LENGTH = 300

/**
 * Sends requests to one RomM server and turns every failure into a [RommException].
 *
 * Authenticated calls take their bearer token from [tokens]; on a 401 the transport tells the token source
 * and throws [RommException.Unauthorized]. [tokens] is null only for transports that make no authenticated
 * calls (the device flow).
 */
internal class RommTransport(
    private val http: HttpClient,
    baseUrl: String,
    private val tokens: RommTokenSource?,
) {
    private val baseUrl = baseUrl.trimEnd('/')

    suspend fun send(
        method: HttpMethod,
        path: String,
        authenticated: Boolean = true,
        configure: HttpRequestBuilder.() -> Unit = {},
    ): HttpResponse {
        val token = if (authenticated) requireToken() else null
        val response =
            try {
                http.request("$baseUrl$path") {
                    this.method = method
                    token?.let { bearerAuth(it) }
                    configure()
                }
            } catch (e: IOException) {
                throw RommException.Network(e)
            }
        if (!response.status.isSuccess()) throw toException(response, token)
        return response
    }

    suspend inline fun <reified B : Any> sendJson(
        method: HttpMethod,
        path: String,
        body: B,
        authenticated: Boolean = true,
    ): HttpResponse =
        send(method, path, authenticated) {
            contentType(ContentType.Application.Json)
            setBody(body)
        }

    /** Reads a successful response as [T]; a body that does not match is reported as [RommException]. */
    suspend inline fun <reified T> decode(response: HttpResponse): T =
        try {
            response.body()
        } catch (e: IOException) {
            throw RommException.Network(e)
        } catch (e: ContentConvertException) {
            throw malformedBody(response, e)
        } catch (e: SerializationException) {
            throw malformedBody(response, e)
        }

    suspend fun readBytes(response: HttpResponse): ByteArray =
        try {
            response.bodyAsBytes()
        } catch (e: IOException) {
            throw RommException.Network(e)
        }

    fun malformedBody(
        response: HttpResponse,
        cause: Throwable,
    ): RommException = RommException.UnexpectedResponse(response.status.value, "malformed body: ${cause.message}")

    private suspend fun requireToken(): String {
        val source = checkNotNull(tokens) { "This transport has no token source" }
        val token = source.accessToken()
        if (token == null) {
            source.onUnauthorized(null)
            throw RommException.Unauthorized("no access token")
        }
        return token
    }

    private suspend fun toException(
        response: HttpResponse,
        token: String?,
    ): RommException {
        val status = response.status
        val detail = errorDetail(response.bodyAsText())
        return when {
            status == HttpStatusCode.Unauthorized -> {
                tokens?.onUnauthorized(token)
                RommException.Unauthorized(detail)
            }

            status == HttpStatusCode.Forbidden -> {
                RommException.Forbidden(detail)
            }

            status == HttpStatusCode.NotFound -> {
                RommException.NotFound(detail)
            }

            status == HttpStatusCode.Conflict -> {
                RommException.Conflict(detail)
            }

            status == HttpStatusCode.TooManyRequests -> {
                RommException.RateLimited(detail)
            }

            status == HttpStatusCode.BadRequest || status == HttpStatusCode.UnprocessableEntity -> {
                RommException.InvalidRequest(status.value, detail)
            }

            status.value >= HttpStatusCode.InternalServerError.value -> {
                RommException.ServerError(status.value, detail)
            }

            else -> {
                RommException.UnexpectedResponse(status.value, detail)
            }
        }
    }
}

/** RomM's FastAPI error body is `{"detail": "text"}`, or `{"detail": {"message": ...}}`, or a validation list. */
internal fun errorDetail(body: String): String {
    val root = runCatching { RommJson.parseToJsonElement(body) }.getOrNull()
    val text =
        when (val detail = (root as? JsonObject)?.get("detail")) {
            null -> body.ifBlank { "no detail" }
            is JsonPrimitive -> detail.contentOrNull ?: detail.toString()
            is JsonObject -> (detail["message"] as? JsonPrimitive)?.contentOrNull ?: detail.toString()
            else -> detail.toString()
        }
    return text.take(MAX_DETAIL_LENGTH)
}
