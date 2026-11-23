package dev.rommsavesyncer.core.romm

import dev.rommsavesyncer.testing.harness.RommEnvironment
import dev.rommsavesyncer.testing.harness.RommServer
import kotlinx.coroutines.runBlocking
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNotEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows
import kotlin.time.Clock
import kotlin.time.Duration.Companion.days

@RommServer
class DeviceAuthIntegrationTest {
    @Test
    fun theHarnessServerPassesTheVersionGate(env: RommEnvironment) =
        runBlocking<Unit> {
            val info = unauthenticatedApi(env).requireSupportedServer()

            assertEquals("5.4.1", info.version)
            assertEquals(RommVersion.MINIMUM, info.parsedVersion)
        }

    @Test
    fun anAdminApprovedCodeEndsWithATokenBoundToTheNewDevice(env: RommEnvironment) =
        runBlocking<Unit> {
            val device = SimulatedDevice.pair(env, "Approved device", expiresIn = "30d")

            val expiresAt = checkNotNull(device.token.expiresAt)
            val now = Clock.System.now()
            assertTrue(expiresAt > now + 29.days && expiresAt < now + 31.days, "expires in about 30 days: $expiresAt")
            assertFalse(device.token.isExpiredAt(now))
            assertEquals(RommScopes.REQUIRED.sorted(), device.token.scopes.sorted())
            val knownDevices =
                env.admin
                    .get("/api/devices")
                    .requireSuccess("Listing devices")
                    .json()
                    .toString()
            assertTrue(device.deviceId in knownDevices, "RomM created the device on approval")
            assertEquals(emptyList<RommPlaySession>(), device.client.listPlaySessions(deviceId = device.deviceId))
        }

    @Test
    fun aDeniedCodeEndsTheFlowWithTheTypedError(env: RommEnvironment) =
        runBlocking<Unit> {
            val request = DeviceAuthRequest("it-denied", "Denied device", "RommSaveSyncer-IT", "linux")

            assertThrows<RommException.DeviceAccessDenied> {
                DeviceAuthorizer(unauthenticatedApi(env)).authorize(request) { env.admin.denyDevice(it.userCode) }
            }
        }

    @Test
    fun anUnknownDeviceCodeIsReportedAsExpired(env: RommEnvironment) =
        runBlocking<Unit> {
            assertEquals(DevicePoll.Expired, unauthenticatedApi(env).pollDeviceAuth("not-a-real-device-code"))
        }

    @Test
    fun aCodeNobodyHasDecidedYetIsPendingAndAFastRepeatPollIsToldToSlowDown(env: RommEnvironment) =
        runBlocking<Unit> {
            val api = unauthenticatedApi(env)
            val challenge = api.startDeviceAuth(DeviceAuthRequest("it-pending", "Pending device", "RommSaveSyncer-IT"))

            assertEquals(DevicePoll.Pending, api.pollDeviceAuth(challenge.deviceCode))
            assertEquals(DevicePoll.SlowDown, api.pollDeviceAuth(challenge.deviceCode))
            assertEquals(POLL_INTERVAL_SECONDS, challenge.interval)
            assertEquals(CODE_LIFETIME_SECONDS, challenge.expiresIn)
        }

    @Test
    fun registeringADeviceTwiceReturnsTheSameDeviceUnlessADuplicateIsAsked(env: RommEnvironment) =
        runBlocking<Unit> {
            val device = SimulatedDevice.pair(env, "Registering device")
            val registration =
                DeviceRegistration(
                    name = "Registered PC",
                    platform = "linux",
                    client = "RommSaveSyncer-IT",
                    clientVersion = "0.0.0",
                    macAddress = "02:00:00:00:00:01",
                    hostname = "it-host",
                    syncMode = SyncMode.API,
                    capabilities = mapOf("remote_install" to false),
                )

            val first = device.client.registerDevice(registration)
            val second = device.client.registerDevice(registration)
            val duplicate = device.client.registerDevice(registration.copy(allowDuplicate = true))

            assertFalse(first.alreadyExisted)
            assertTrue(second.alreadyExisted)
            assertEquals(first.deviceId, second.deviceId)
            assertNotEquals(first.deviceId, duplicate.deviceId)
            assertNotEquals(device.deviceId, first.deviceId)
        }

    @Test
    fun aGarbageTokenIsATypedUnauthorizedAndTheTokenSourceIsTold(env: RommEnvironment) =
        runBlocking<Unit> {
            val tokens = RecordingTokenSource("not-a-real-token")
            val client = clientWithToken(env, tokens)

            assertThrows<RommException.Unauthorized> { client.listSaves() }

            assertEquals(listOf<String?>("not-a-real-token"), tokens.rejected)
        }

    @Test
    fun aRevokedDeviceTokenIsUnauthorizedFromThenOn(env: RommEnvironment) =
        runBlocking<Unit> {
            val device = SimulatedDevice.pair(env, "Revoked device")
            device.client.listSaves()

            env.admin.revokeDeviceTokens(device.deviceId)

            assertThrows<RommException.Unauthorized> { device.client.listSaves() }
            assertEquals(listOf<String?>(device.token.accessToken), device.tokens.rejected)
        }

    @Test
    fun aTokenWithoutTheNeededScopeIsForbiddenNotUnauthorized(env: RommEnvironment) =
        runBlocking<Unit> {
            val device = SimulatedDevice.pair(env, "Read only device", scopes = listOf("me.read", "assets.read"))

            assertThrows<RommException.Forbidden> { device.client.registerDevice(DeviceRegistration(name = "x")) }

            assertTrue(device.tokens.rejected.isEmpty())
        }

    @Test
    fun clearingTheTokenSourceMakesEveryCallUnauthorizedWithoutARequest(env: RommEnvironment) =
        runBlocking<Unit> {
            val tokens = RecordingTokenSource(null)

            assertThrows<RommException.Unauthorized> { clientWithToken(env, tokens).listSaves() }

            assertEquals(listOf<String?>(null), tokens.rejected)
        }

    private companion object {
        // RomM 5.4.1's device flow: `POLL_DEFAULT_INTERVAL_SECONDS` and `PENDING_TTL_SECONDS` in utils/device_auth.py.
        const val POLL_INTERVAL_SECONDS = 5
        const val CODE_LIFETIME_SECONDS = 600
    }
}
