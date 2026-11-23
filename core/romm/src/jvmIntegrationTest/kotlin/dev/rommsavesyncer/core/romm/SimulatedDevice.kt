package dev.rommsavesyncer.core.romm

import dev.rommsavesyncer.testing.harness.RommEnvironment
import kotlinx.coroutines.runBlocking
import java.util.UUID

private val http = createRommHttpClient()

/** A token source that remembers every token RomM rejected, as the app's secret store would react to. */
class RecordingTokenSource(
    var token: String?,
) : RommTokenSource {
    val rejected = mutableListOf<String?>()

    override suspend fun accessToken(): String? = token

    override suspend fun onUnauthorized(rejectedToken: String?) {
        rejected += rejectedToken
    }
}

/**
 * One simulated Device: paired with the harness RomM through the real device flow, the admin approving the
 * code headlessly, and talking to RomM with the token that flow produced.
 */
class SimulatedDevice private constructor(
    val env: RommEnvironment,
    val name: String,
    val token: DeviceAuthToken,
) {
    val deviceId: String get() = token.deviceId
    val tokens = RecordingTokenSource(token.accessToken)
    val client: RommClient = KtorRommClient(http, env.baseUrl, tokens)

    companion object {
        /** Pairs a new device named [name] that is granted [scopes] and whose token lasts [expiresIn]. */
        fun pair(
            env: RommEnvironment,
            name: String,
            scopes: List<String> = RommScopes.REQUIRED,
            expiresIn: String? = null,
        ): SimulatedDevice =
            runBlocking {
                val request =
                    DeviceAuthRequest(
                        clientDeviceIdentifier = "it-${UUID.randomUUID()}",
                        name = name,
                        client = "RommSaveSyncer-IT",
                        platform = "linux",
                        clientVersion = "0.0.0",
                        requestedScopes = scopes,
                    )
                val token =
                    DeviceAuthorizer(KtorDeviceAuthApi(http, env.baseUrl)).authorize(request) { challenge ->
                        env.admin.approveDevice(challenge.userCode, scopes, name, expiresIn)
                    }
                SimulatedDevice(env, name, token)
            }
    }
}

/** A client for [env] that signs in with whatever [token] says, for the paths without a device. */
fun clientWithToken(
    env: RommEnvironment,
    token: RecordingTokenSource,
): RommClient = KtorRommClient(http, env.baseUrl, token)

fun unauthenticatedApi(env: RommEnvironment): KtorDeviceAuthApi = KtorDeviceAuthApi(http, env.baseUrl)
