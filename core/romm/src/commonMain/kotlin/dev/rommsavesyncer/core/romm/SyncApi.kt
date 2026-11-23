package dev.rommsavesyncer.core.romm

import io.ktor.http.HttpMethod
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlin.time.Instant

/** What the negotiation tells a device to do with one save (`handler/sync/comparison.py`, `SyncAction`). */
@Serializable
enum class SyncAction {
    @SerialName("upload")
    UPLOAD,

    @SerialName("download")
    DOWNLOAD,

    @SerialName("conflict")
    CONFLICT,

    @SerialName("no_op")
    NO_OP,

    @SerialName("delete")
    DELETE,
}

/** One save the device holds. A null [slot] is an archival save, which RomM never pairs with slotted saves. */
@Serializable
data class ClientSaveState(
    @SerialName("rom_id") val romId: Int,
    @SerialName("file_name") val fileName: String,
    val slot: String?,
    val emulator: String? = null,
    @SerialName("content_hash") val contentHash: String? = null,
    @SerialName("updated_at") @Serializable(with = RommInstantSerializer::class) val updatedAt: Instant,
    @SerialName("file_size_bytes") val fileSizeBytes: Long,
)

/** The body of `POST /api/sync/negotiate`. [deviceId] may be left out for a device-bound token. */
@Serializable
data class NegotiateRequest(
    @SerialName("device_id") val deviceId: String? = null,
    val saves: List<ClientSaveState>,
    @SerialName("rom_ids") val romIds: List<Int>? = null,
    @SerialName("restore_unlisted") val restoreUnlisted: Boolean = false,
    val emulators: List<String>? = null,
)

@Serializable
data class SyncOperation(
    val action: SyncAction,
    @SerialName("rom_id") val romId: Int,
    @SerialName("save_id") val saveId: Int? = null,
    @SerialName("file_name") val fileName: String,
    val slot: String? = null,
    val emulator: String? = null,
    val reason: String,
    @SerialName("server_updated_at")
    @Serializable(with = RommInstantSerializer::class)
    val serverUpdatedAt: Instant? = null,
    @SerialName("server_content_hash") val serverContentHash: String? = null,
)

@Serializable
data class NegotiateResponse(
    @SerialName("session_id") val sessionId: Int,
    val operations: List<SyncOperation>,
    @SerialName("total_upload") val totalUpload: Int,
    @SerialName("total_download") val totalDownload: Int,
    @SerialName("total_conflict") val totalConflict: Int,
    @SerialName("total_no_op") val totalNoOp: Int,
    @SerialName("total_delete") val totalDelete: Int,
)

/** A play session as sent to RomM. `endTime` must be after `startTime`; RomM truncates both to whole seconds. */
@Serializable
data class PlaySessionEntry(
    @SerialName("rom_id") val romId: Int?,
    @SerialName("save_slot") val saveSlot: String? = null,
    @SerialName("start_time") @Serializable(with = RommInstantSerializer::class) val startTime: Instant,
    @SerialName("end_time") @Serializable(with = RommInstantSerializer::class) val endTime: Instant,
    @SerialName("duration_ms") val durationMs: Long,
)

/** The body of `POST /api/sync/sessions/{id}/complete`; [playSessions] are recorded against the session's device. */
@Serializable
data class CompleteSessionRequest(
    @SerialName("operations_completed") val operationsCompleted: Int = 0,
    @SerialName("operations_failed") val operationsFailed: Int = 0,
    @SerialName("play_sessions") val playSessions: List<PlaySessionEntry>? = null,
)

/** `models/sync_session.py`, `SyncSessionStatus`; unlike [SyncAction] these travel in upper case. */
@Serializable
enum class SyncSessionStatus {
    @SerialName("PENDING")
    PENDING,

    @SerialName("IN_PROGRESS")
    IN_PROGRESS,

    @SerialName("COMPLETED")
    COMPLETED,

    @SerialName("FAILED")
    FAILED,

    @SerialName("CANCELLED")
    CANCELLED,
}

@Serializable
data class SyncSession(
    val id: Int,
    @SerialName("device_id") val deviceId: String,
    val status: SyncSessionStatus,
    @SerialName("operations_planned") val operationsPlanned: Int,
    @SerialName("operations_completed") val operationsCompleted: Int,
    @SerialName("operations_failed") val operationsFailed: Int,
    @SerialName("error_message") val errorMessage: String? = null,
)

@Serializable
data class CompleteSessionResponse(
    val session: SyncSession,
    @SerialName("play_session_ingest") val playSessionIngest: PlaySessionIngestResponse? = null,
)

interface RommSyncApi {
    /**
     * `POST /api/sync/negotiate` (scopes `assets.read` and `devices.read`) opens a sync session and lists what
     * RomM would do. Advisory only: the engine decides from its own baselines.
     */
    suspend fun negotiate(request: NegotiateRequest): NegotiateResponse

    /** `POST /api/sync/sessions/{id}/complete` (scope `devices.write`); a session can be completed once. */
    suspend fun completeSession(
        sessionId: Int,
        request: CompleteSessionRequest = CompleteSessionRequest(),
    ): CompleteSessionResponse
}

internal class KtorSyncApi(
    private val transport: RommTransport,
) : RommSyncApi {
    override suspend fun negotiate(request: NegotiateRequest): NegotiateResponse =
        transport.decode(transport.sendJson(HttpMethod.Post, "/api/sync/negotiate", request))

    override suspend fun completeSession(
        sessionId: Int,
        request: CompleteSessionRequest,
    ): CompleteSessionResponse =
        transport.decode(transport.sendJson(HttpMethod.Post, "/api/sync/sessions/$sessionId/complete", request))
}
