package dev.rommsavesyncer.core.romm

import io.ktor.client.request.parameter
import io.ktor.http.HttpMethod
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlin.time.Instant

/** How RomM treated one entry of an ingested batch (`PlaySessionIngestResult`). */
@Serializable
enum class PlaySessionIngestStatus {
    @SerialName("created")
    CREATED,

    @SerialName("duplicate")
    DUPLICATE,

    @SerialName("error")
    ERROR,
}

@Serializable
data class PlaySessionIngestResult(
    val index: Int,
    val status: PlaySessionIngestStatus,
    val id: Int? = null,
    val detail: String? = null,
)

@Serializable
data class PlaySessionIngestResponse(
    val results: List<PlaySessionIngestResult>,
    @SerialName("created_count") val createdCount: Int,
    @SerialName("skipped_count") val skippedCount: Int,
)

/** A play session RomM has stored (`PlaySessionSchema`). */
@Serializable
data class RommPlaySession(
    val id: Int,
    @SerialName("device_id") val deviceId: String? = null,
    @SerialName("rom_id") val romId: Int? = null,
    @SerialName("save_slot") val saveSlot: String? = null,
    @SerialName("start_time") @Serializable(with = RommInstantSerializer::class) val startTime: Instant,
    @SerialName("end_time") @Serializable(with = RommInstantSerializer::class) val endTime: Instant,
    @SerialName("duration_ms") val durationMs: Long,
)

@Serializable
private data class PlaySessionIngestRequest(
    @SerialName("device_id") val deviceId: String? = null,
    val sessions: List<PlaySessionEntry>,
)

interface RommPlaySessionsApi {
    /**
     * `POST /api/play-sessions` (scope `roms.user.write`), 1 to 100 sessions. Entries RomM already has (same
     * ROM and start time for the device) come back as duplicates, so a retry is safe.
     */
    suspend fun ingestPlaySessions(
        sessions: List<PlaySessionEntry>,
        deviceId: String? = null,
    ): PlaySessionIngestResponse

    /**
     * `GET /api/play-sessions` (scope `roms.user.read`). A device-bound token may only list its own device,
     * so pass that [deviceId]; other filters need `devices.read` as well.
     */
    suspend fun listPlaySessions(
        deviceId: String? = null,
        romId: Int? = null,
    ): List<RommPlaySession>
}

internal class KtorPlaySessionsApi(
    private val transport: RommTransport,
) : RommPlaySessionsApi {
    override suspend fun ingestPlaySessions(
        sessions: List<PlaySessionEntry>,
        deviceId: String?,
    ): PlaySessionIngestResponse =
        transport.decode(
            transport.sendJson(HttpMethod.Post, "/api/play-sessions", PlaySessionIngestRequest(deviceId, sessions)),
        )

    override suspend fun listPlaySessions(
        deviceId: String?,
        romId: Int?,
    ): List<RommPlaySession> {
        val response =
            transport.send(HttpMethod.Get, "/api/play-sessions") {
                parameter("device_id", deviceId)
                parameter("rom_id", romId)
            }
        return transport.decode(response)
    }
}
