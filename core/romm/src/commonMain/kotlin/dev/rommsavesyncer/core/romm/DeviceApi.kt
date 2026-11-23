package dev.rommsavesyncer.core.romm

import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlin.time.Instant

/** How a device exchanges saves with RomM; RommSaveSyncer uses [API] (`backend/models/device.py`). */
@Serializable
enum class SyncMode {
    @SerialName("api")
    API,

    @SerialName("file_transfer")
    FILE_TRANSFER,

    @SerialName("push_pull")
    PUSH_PULL,
}

/**
 * The body of `POST /api/devices`. RomM matches an existing device by MAC address, hostname and platform, so
 * registering again returns the same device (`allowExisting`) unless [allowDuplicate] asks for a new one.
 */
@Serializable
data class DeviceRegistration(
    val name: String? = null,
    val platform: String? = null,
    val client: String? = null,
    @SerialName("client_version") val clientVersion: String? = null,
    @SerialName("mac_address") val macAddress: String? = null,
    val hostname: String? = null,
    @SerialName("sync_mode") val syncMode: SyncMode? = null,
    val capabilities: Map<String, Boolean>? = null,
    @SerialName("allow_existing") val allowExisting: Boolean = true,
    @SerialName("allow_duplicate") val allowDuplicate: Boolean = false,
    @SerialName("reset_syncs") val resetSyncs: Boolean = false,
)

@Serializable
private data class DeviceCreateResponse(
    @SerialName("device_id") val deviceId: String,
    val name: String? = null,
    @SerialName("created_at") @Serializable(with = RommInstantSerializer::class) val createdAt: Instant,
)

/** The device RomM knows; [alreadyExisted] is true when the registration matched a device from before (HTTP 200). */
data class RegisteredDevice(
    val deviceId: String,
    val name: String?,
    val createdAt: Instant,
    val alreadyExisted: Boolean,
)

interface RommDeviceApi {
    /** `POST /api/devices` (scope `devices.write`): 201 for a new device, 200 for one that was already registered. */
    suspend fun registerDevice(registration: DeviceRegistration): RegisteredDevice
}

internal class KtorDeviceApi(
    private val transport: RommTransport,
) : RommDeviceApi {
    override suspend fun registerDevice(registration: DeviceRegistration): RegisteredDevice {
        val response = transport.sendJson(HttpMethod.Post, "/api/devices", registration)
        val created = transport.decode<DeviceCreateResponse>(response)
        return RegisteredDevice(
            deviceId = created.deviceId,
            name = created.name,
            createdAt = created.createdAt,
            alreadyExisted = response.status == HttpStatusCode.OK,
        )
    }
}
