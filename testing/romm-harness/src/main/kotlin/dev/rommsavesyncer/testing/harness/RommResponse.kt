package dev.rommsavesyncer.testing.harness

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

/** An HTTP response from RomM; [json] parses the body on demand. */
class RommResponse(
    val status: Int,
    val body: String,
) {
    val isSuccess: Boolean get() = status in HTTP_SUCCESS

    fun json(): JsonElement = Json.parseToJsonElement(body)

    fun jsonObject(): JsonObject = json().jsonObject

    /** Returns this response, or fails with the status and body, so a failing bootstrap step explains itself. */
    fun requireSuccess(what: String): RommResponse {
        check(isSuccess) { "$what failed: HTTP $status $body" }
        return this
    }

    override fun toString(): String = "HTTP $status $body"

    private companion object {
        val HTTP_SUCCESS = 200..299
    }
}
