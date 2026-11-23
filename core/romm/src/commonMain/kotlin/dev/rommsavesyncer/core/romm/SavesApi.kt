package dev.rommsavesyncer.core.romm

import io.ktor.client.request.forms.MultiPartFormDataContent
import io.ktor.client.request.forms.formData
import io.ktor.client.request.parameter
import io.ktor.client.request.setBody
import io.ktor.http.Headers
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpMethod
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlin.time.Instant

/** A save RomM stores (`SaveSchema`). [slot] is null for archival saves; slotted saves are versioned. */
@Serializable
data class RommSave(
    val id: Int,
    @SerialName("rom_id") val romId: Int,
    @SerialName("file_name") val fileName: String,
    @SerialName("file_size_bytes") val fileSizeBytes: Long,
    val emulator: String? = null,
    val slot: String? = null,
    @SerialName("content_hash") val contentHash: String? = null,
    @SerialName("origin_device_id") val originDeviceId: String? = null,
    @SerialName("created_at") @Serializable(with = RommInstantSerializer::class) val createdAt: Instant,
    @SerialName("updated_at") @Serializable(with = RommInstantSerializer::class) val updatedAt: Instant,
    @SerialName("device_syncs") val deviceSyncs: List<DeviceSync> = emptyList(),
)

/** Whether one device holds a save's current version (`DeviceSyncSchema`). */
@Serializable
data class DeviceSync(
    @SerialName("device_id") val deviceId: String,
    @SerialName("device_name") val deviceName: String? = null,
    @SerialName("last_synced_at") @Serializable(with = RommInstantSerializer::class) val lastSyncedAt: Instant,
    @SerialName("is_untracked") val isUntracked: Boolean = false,
    @SerialName("is_current") val isCurrent: Boolean = false,
)

/** The bytes of a save and the file name RomM should store them under. */
class SaveFile(
    val fileName: String,
    val bytes: ByteArray,
)

/**
 * One `POST /api/saves`. Everything but the file travels as a query parameter, not as a form field.
 *
 * [slot] is `autosave` for a game's current save (ADR 0003) and null for an archival copy. With a [deviceId],
 * RomM refuses (409) a slotted upload when the slot has a newer save than this device last synced, unless
 * [overwrite] is set. [contentHash] is the MD5 of the bytes; RomM stores it as the device's sync baseline.
 * [autocleanupLimit] keeps only that many versions in the slot.
 */
data class SaveUpload(
    val romId: Int,
    val file: SaveFile,
    val slot: String?,
    val emulator: String? = null,
    val deviceId: String? = null,
    val sessionId: Int? = null,
    val contentHash: String? = null,
    val overwrite: Boolean = false,
    val autocleanupLimit: Int? = null,
)

interface RommSavesApi {
    /** `GET /api/saves` (scope `assets.read`), optionally narrowed to one ROM and slot. */
    suspend fun listSaves(
        romId: Int? = null,
        slot: String? = null,
        deviceId: String? = null,
    ): List<RommSave>

    /**
     * `POST /api/saves` (scope `assets.write`, plus `devices.write` with a device). Throws
     * [RommException.Conflict] when the slot guard refuses a stale upload.
     */
    suspend fun uploadSave(upload: SaveUpload): RommSave

    /**
     * `GET /api/saves/{id}/content` (scope `assets.read`). With a [deviceId] and [optimistic], RomM already
     * records the device as synced to the version it serves; [confirmDownload] then supplies the hash.
     */
    suspend fun downloadSave(
        saveId: Int,
        deviceId: String? = null,
        sessionId: Int? = null,
        optimistic: Boolean = true,
    ): ByteArray

    /** `POST /api/saves/{id}/downloaded` (scope `devices.write`): the device now holds [contentHash]. */
    suspend fun confirmDownload(
        saveId: Int,
        deviceId: String,
        contentHash: String?,
    ): RommSave
}

@Serializable
private data class ConfirmDownloadRequest(
    @SerialName("device_id") val deviceId: String,
    @SerialName("content_hash") val contentHash: String? = null,
)

internal class KtorSavesApi(
    private val transport: RommTransport,
) : RommSavesApi {
    override suspend fun listSaves(
        romId: Int?,
        slot: String?,
        deviceId: String?,
    ): List<RommSave> {
        val response =
            transport.send(HttpMethod.Get, "/api/saves") {
                parameter("rom_id", romId)
                parameter("slot", slot)
                parameter("device_id", deviceId)
            }
        return transport.decode(response)
    }

    override suspend fun uploadSave(upload: SaveUpload): RommSave {
        val response =
            transport.send(HttpMethod.Post, "/api/saves") {
                parameter("rom_id", upload.romId)
                parameter("slot", upload.slot)
                parameter("emulator", upload.emulator)
                parameter("device_id", upload.deviceId)
                parameter("session_id", upload.sessionId)
                parameter("content_hash", upload.contentHash)
                parameter("overwrite", upload.overwrite)
                upload.autocleanupLimit?.let {
                    parameter("autocleanup", true)
                    parameter("autocleanup_limit", it)
                }
                setBody(MultiPartFormDataContent(saveFileForm(upload.file)))
            }
        return transport.decode(response)
    }

    override suspend fun downloadSave(
        saveId: Int,
        deviceId: String?,
        sessionId: Int?,
        optimistic: Boolean,
    ): ByteArray {
        val response =
            transport.send(HttpMethod.Get, "/api/saves/$saveId/content") {
                parameter("device_id", deviceId)
                parameter("session_id", sessionId)
                parameter("optimistic", optimistic)
            }
        return transport.readBytes(response)
    }

    override suspend fun confirmDownload(
        saveId: Int,
        deviceId: String,
        contentHash: String?,
    ): RommSave =
        transport.decode(
            transport.sendJson(
                HttpMethod.Post,
                "/api/saves/$saveId/downloaded",
                ConfirmDownloadRequest(deviceId, contentHash),
            ),
        )

    private fun saveFileForm(file: SaveFile) =
        formData {
            append(
                SAVE_FILE_PART,
                file.bytes,
                Headers.build {
                    append(HttpHeaders.ContentType, "application/octet-stream")
                    append(HttpHeaders.ContentDisposition, "filename=\"${quotedFileName(file.fileName)}\"")
                },
            )
        }

    /** Keeps a file name inside its quoted header value: CR and LF become spaces, `\` is doubled, `"` is `%22`. */
    private fun quotedFileName(name: String): String =
        name
            .replace("\r", " ")
            .replace("\n", " ")
            .replace("\\", "\\\\")
            .replace("\"", "%22")

    private companion object {
        const val SAVE_FILE_PART = "saveFile"
    }
}
