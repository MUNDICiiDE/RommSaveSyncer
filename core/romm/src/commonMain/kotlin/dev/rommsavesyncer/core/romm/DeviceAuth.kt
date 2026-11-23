package dev.rommsavesyncer.core.romm

import io.ktor.client.HttpClient
import io.ktor.http.HttpMethod
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant
import kotlin.time.TimeMark
import kotlin.time.TimeSource

/** The scopes the device token needs: the ones every endpoint RommSaveSyncer calls requires. */
object RommScopes {
    val REQUIRED: List<String> =
        listOf(
            "me.read",
            "roms.read",
            "roms.user.read",
            "roms.user.write",
            "assets.read",
            "assets.write",
            "devices.read",
            "devices.write",
        )
}

/**
 * The body of `POST /api/auth/device/init`. [clientDeviceIdentifier] is a stable id for this install, so
 * pairing the same device again updates its RomM device instead of adding another.
 */
@Serializable
data class DeviceAuthRequest(
    @SerialName("client_device_identifier") val clientDeviceIdentifier: String,
    val name: String,
    val client: String,
    val platform: String? = null,
    @SerialName("client_version") val clientVersion: String? = null,
    @SerialName("requested_scopes") val requestedScopes: List<String> = RommScopes.REQUIRED,
)

/**
 * What the user needs to approve the device. Show [userCode] and the page at [verificationPathComplete]
 * (joined to the RomM server's origin); the app polls with [deviceCode], which must stay secret.
 */
@Serializable
data class DeviceAuthChallenge(
    @SerialName("device_code") val deviceCode: String,
    @SerialName("user_code") val userCode: String,
    @SerialName("verification_path") val verificationPath: String,
    @SerialName("verification_path_complete") val verificationPathComplete: String,
    @SerialName("expires_in") val expiresIn: Int,
    val interval: Int,
)

/**
 * The approved device: its RomM [deviceId] and the bearer token bound to it.
 * [expiresAt] is null for a token that never expires.
 */
@Serializable
data class DeviceAuthToken(
    @SerialName("access_token") val accessToken: String,
    @SerialName("device_id") val deviceId: String,
    val scopes: List<String>,
    @SerialName("expires_at") @Serializable(with = RommInstantSerializer::class) val expiresAt: Instant? = null,
) {
    fun isExpiredAt(now: Instant): Boolean = expiresAt != null && expiresAt <= now
}

/** The answer to one poll of `POST /api/auth/device/token`. */
sealed interface DevicePoll {
    data class Approved(
        val token: DeviceAuthToken,
    ) : DevicePoll

    /** `authorization_pending`: the user has not decided yet. */
    data object Pending : DevicePoll

    /** `slow_down`: polled inside the interval; wait longer between polls. */
    data object SlowDown : DevicePoll

    /** `expired_token`: the code is unknown or timed out. */
    data object Expired : DevicePoll

    /** `access_denied`: the user denied the device. */
    data object Denied : DevicePoll
}

@Serializable
private data class DevicePollRequest(
    @SerialName("device_code") val deviceCode: String,
)

/** The unauthenticated endpoints of the device flow (`backend/endpoints/device/auth.py`) and the heartbeat. */
interface RommDeviceAuthApi : RommServerApi {
    /** `POST /api/auth/device/init`: starts pairing. */
    suspend fun startDeviceAuth(request: DeviceAuthRequest): DeviceAuthChallenge

    /** `POST /api/auth/device/token`: RomM answers 400 with the state as `detail` until the user decides. */
    suspend fun pollDeviceAuth(deviceCode: String): DevicePoll
}

class KtorDeviceAuthApi private constructor(
    private val transport: RommTransport,
) : RommDeviceAuthApi,
    RommServerApi by KtorServerApi(transport) {
    constructor(
        http: HttpClient,
        baseUrl: String,
    ) : this(RommTransport(http, baseUrl, tokens = null))

    override suspend fun startDeviceAuth(request: DeviceAuthRequest): DeviceAuthChallenge {
        val response = transport.sendJson(HttpMethod.Post, "/api/auth/device/init", request, authenticated = false)
        return transport.decode(response)
    }

    override suspend fun pollDeviceAuth(deviceCode: String): DevicePoll =
        try {
            val response =
                transport.sendJson(
                    HttpMethod.Post,
                    "/api/auth/device/token",
                    DevicePollRequest(deviceCode),
                    authenticated = false,
                )
            DevicePoll.Approved(transport.decode(response))
        } catch (e: RommException.InvalidRequest) {
            pollStateOf(e)
        }

    private fun pollStateOf(error: RommException.InvalidRequest): DevicePoll =
        when (error.detail) {
            "authorization_pending" -> DevicePoll.Pending
            "slow_down" -> DevicePoll.SlowDown
            "expired_token" -> DevicePoll.Expired
            "access_denied" -> DevicePoll.Denied
            else -> throw error
        }
}

/** Time for the polling loop, so tests can run it without waiting. */
interface PollTimer {
    fun markNow(): TimeMark

    suspend fun delay(duration: Duration)
}

object SystemPollTimer : PollTimer {
    override fun markNow(): TimeMark = TimeSource.Monotonic.markNow()

    override suspend fun delay(duration: Duration) = kotlinx.coroutines.delay(duration)
}

/**
 * The device flow (RFC 8628, as RomM 5.4.1 implements it): refuse an unsupported server, start pairing,
 * show the code, then poll until the user approves. Polls every `interval` seconds, [SLOW_DOWN_STEP]
 * longer after each `slow_down`.
 */
class DeviceAuthorizer(
    private val api: RommDeviceAuthApi,
    private val timer: PollTimer = SystemPollTimer,
) {
    /**
     * Runs the flow. [onChallenge] is called once with the code to show to the user.
     * Throws [RommException.UnsupportedServer] for an old server, [RommException.DeviceCodeExpired] or
     * [RommException.DeviceAccessDenied] when the flow ends without a token, and the usual [RommException]s
     * for transport failures.
     */
    suspend fun authorize(
        request: DeviceAuthRequest,
        onChallenge: suspend (DeviceAuthChallenge) -> Unit,
    ): DeviceAuthToken {
        api.requireSupportedServer()
        val challenge = api.startDeviceAuth(request)
        onChallenge(challenge)
        val started = timer.markNow()
        val lifetime = challenge.expiresIn.seconds
        var interval = challenge.interval.seconds
        while (true) {
            timer.delay(interval)
            val outOfTime = started.elapsedNow() >= lifetime
            when (val poll = if (outOfTime) DevicePoll.Expired else api.pollDeviceAuth(challenge.deviceCode)) {
                is DevicePoll.Approved -> return poll.token
                DevicePoll.Pending -> Unit
                DevicePoll.SlowDown -> interval += SLOW_DOWN_STEP
                DevicePoll.Expired -> throw RommException.DeviceCodeExpired()
                DevicePoll.Denied -> throw RommException.DeviceAccessDenied()
            }
        }
    }

    companion object {
        /** RFC 8628 section 3.5: add five seconds to the interval after `slow_down`. */
        val SLOW_DOWN_STEP: Duration = 5.seconds
    }
}
