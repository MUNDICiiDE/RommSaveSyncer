package dev.rommsavesyncer.core.romm

import io.ktor.http.HttpMethod
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** What `GET /api/heartbeat` says about the server. */
data class RommServerInfo(
    /** The raw version string, such as `5.4.1`. */
    val version: String,
    val showSetupWizard: Boolean,
) {
    val parsedVersion: RommVersion? get() = RommVersion.parse(version)
}

interface RommServerApi {
    /** `GET /api/heartbeat` (no credentials needed). Does not check the version. */
    suspend fun serverInfo(): RommServerInfo

    /** [serverInfo], or [RommException.UnsupportedServer] when RomM is older than [MINIMUM_ROMM_VERSION]. */
    suspend fun requireSupportedServer(): RommServerInfo
}

@Serializable
private data class HeartbeatResponse(
    @SerialName("SYSTEM") val system: HeartbeatSystem,
)

@Serializable
private data class HeartbeatSystem(
    @SerialName("VERSION") val version: String,
    @SerialName("SHOW_SETUP_WIZARD") val showSetupWizard: Boolean = false,
)

internal class KtorServerApi(
    private val transport: RommTransport,
) : RommServerApi {
    override suspend fun serverInfo(): RommServerInfo {
        val response = transport.send(HttpMethod.Get, "/api/heartbeat", authenticated = false)
        val system = transport.decode<HeartbeatResponse>(response).system
        return RommServerInfo(system.version, system.showSetupWizard)
    }

    override suspend fun requireSupportedServer(): RommServerInfo {
        val info = serverInfo()
        val version = info.parsedVersion
        if (version == null || version < RommVersion.MINIMUM) throw RommException.UnsupportedServer(info.version)
        return info
    }
}
