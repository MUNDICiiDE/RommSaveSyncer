package dev.rommsavesyncer.core.romm

import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonPrimitive
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant
import kotlin.time.TimeMark

private val APPROVED =
    DeviceAuthToken("secret-token", "dev-1", listOf("assets.read"), Instant.parse("2027-01-01T00:00:00Z"))

private val CHALLENGE =
    DeviceAuthChallenge(
        deviceCode = "device-code",
        userCode = "ABCD-EFGH",
        verificationPath = "/pair/device",
        verificationPathComplete = "/pair/device?user_code=ABCD-EFGH",
        expiresIn = 600,
        interval = 5,
    )

private val REQUEST = DeviceAuthRequest("install-1", "Windows PC", "RommSaveSyncer", "windows", "0.1.0")

/** A clock that only moves when the code under test waits, and remembers each wait. */
private class FakeTimer : PollTimer {
    var elapsed: Duration = Duration.ZERO
    val delays = mutableListOf<Duration>()

    override fun markNow(): TimeMark {
        val startedAt = elapsed
        return object : TimeMark {
            override fun elapsedNow(): Duration = elapsed - startedAt
        }
    }

    override suspend fun delay(duration: Duration) {
        delays += duration
        elapsed += duration
    }
}

/** A device-flow API that answers each poll from [polls] in order. */
private class ScriptedAuthApi(
    private val polls: MutableList<DevicePoll>,
    private val version: String = "5.4.1",
) : RommDeviceAuthApi {
    var started = 0
    var polled = 0

    override suspend fun serverInfo() = RommServerInfo(version, showSetupWizard = false)

    override suspend fun requireSupportedServer(): RommServerInfo {
        val supported = RommVersion.parse(version)?.let { it >= RommVersion.MINIMUM } == true
        if (!supported) throw RommException.UnsupportedServer(version)
        return serverInfo()
    }

    override suspend fun startDeviceAuth(request: DeviceAuthRequest): DeviceAuthChallenge {
        started++
        return CHALLENGE
    }

    override suspend fun pollDeviceAuth(deviceCode: String): DevicePoll {
        polled++
        return polls.removeFirst()
    }
}

class DeviceAuthTest {
    private fun scripted(
        vararg polls: DevicePoll,
        timer: FakeTimer = FakeTimer(),
        version: String = "5.4.1",
    ): Triple<DeviceAuthorizer, ScriptedAuthApi, FakeTimer> {
        val api = ScriptedAuthApi(polls.toMutableList(), version)
        return Triple(DeviceAuthorizer(api, timer), api, timer)
    }

    @Test
    fun startingPairingPostsTheDeviceAndRequestedScopesWithoutCredentials() =
        runSuspending {
            val romm =
                MockRomm {
                    json(
                        """{"device_code":"device-code","user_code":"ABCD-EFGH","verification_path":"/pair/device",
                           "verification_path_complete":"/pair/device?user_code=ABCD-EFGH",
                           "expires_in":600,"interval":5}""",
                        HttpStatusCode.Created,
                    )
                }

            val challenge = romm.deviceAuth.startDeviceAuth(REQUEST)

            assertEquals(CHALLENGE, challenge)
            assertEquals("/api/auth/device/init", romm.request.url.encodedPath)
            assertNull(romm.request.headers[HttpHeaders.Authorization])
            val body = romm.request.bodyJson()
            assertEquals("install-1", body.getValue("client_device_identifier").jsonPrimitive.content)
            assertEquals("Windows PC", body.getValue("name").jsonPrimitive.content)
            assertEquals("RommSaveSyncer", body.getValue("client").jsonPrimitive.content)
            assertEquals("windows", body.getValue("platform").jsonPrimitive.content)
            assertEquals("0.1.0", body.getValue("client_version").jsonPrimitive.content)
            assertEquals(
                RommScopes.REQUIRED,
                body.getValue("requested_scopes").jsonArray.map { it.jsonPrimitive.content },
            )
        }

    @Test
    fun theRequestedScopesCoverEveryEndpointTheClientCalls() {
        val needed =
            setOf(
                "assets.read", // negotiate, list and download saves
                "assets.write", // upload
                "devices.read", // negotiate
                "devices.write", // register, confirm, complete
                "roms.read", // roms and by-hash
                "roms.user.write", // play sessions
                "roms.user.read", // list play sessions
            )

        assertTrue(RommScopes.REQUIRED.containsAll(needed))
    }

    @Test
    fun eachPollStateRommReportsMapsToItsOwnOutcome() =
        runSuspending {
            val states =
                mapOf(
                    "authorization_pending" to DevicePoll.Pending,
                    "slow_down" to DevicePoll.SlowDown,
                    "expired_token" to DevicePoll.Expired,
                    "access_denied" to DevicePoll.Denied,
                )

            for ((detail, expected) in states) {
                val romm = MockRomm { failure(HttpStatusCode.BadRequest, detail) }

                assertEquals(expected, romm.deviceAuth.pollDeviceAuth("device-code"), detail)
                assertEquals("/api/auth/device/token", romm.request.url.encodedPath)
                assertEquals(
                    "device-code",
                    romm.request
                        .bodyJson()
                        .getValue("device_code")
                        .jsonPrimitive.content,
                )
                assertNull(romm.request.headers[HttpHeaders.Authorization])
            }
        }

    @Test
    fun anApprovedPollReturnsTheTokenAndItsExpiry() =
        runSuspending {
            val romm =
                MockRomm {
                    json(
                        """{"access_token":"secret-token","device_id":"dev-1","scopes":["assets.read"],
                           "expires_at":"2027-01-01T00:00:00+00:00"}""",
                    )
                }

            val poll = romm.deviceAuth.pollDeviceAuth("device-code")

            assertEquals(DevicePoll.Approved(APPROVED), poll)
        }

    @Test
    fun aTokenWithoutAnExpiryNeverExpires() =
        runSuspending {
            val romm = MockRomm { json("""{"access_token":"t","device_id":"d","scopes":[],"expires_at":null}""") }

            val token = assertIs<DevicePoll.Approved>(romm.deviceAuth.pollDeviceAuth("c")).token

            assertNull(token.expiresAt)
            assertFalse(token.isExpiredAt(Instant.parse("2100-01-01T00:00:00Z")))
        }

    @Test
    fun aTokenIsExpiredFromItsExpiryOnward() {
        assertFalse(APPROVED.isExpiredAt(Instant.parse("2026-12-31T23:59:59Z")))
        assertTrue(APPROVED.isExpiredAt(Instant.parse("2027-01-01T00:00:00Z")))
    }

    @Test
    fun aBadRequestThatIsNotAPollStateIsStillAnError() =
        runSuspending {
            val romm = MockRomm { failure(HttpStatusCode.BadRequest, "something else") }

            assertFailsWith<RommException.InvalidRequest> { romm.deviceAuth.pollDeviceAuth("device-code") }
        }

    @Test
    fun theFlowWaitsTheIntervalBeforeEachPollUntilApproved() =
        runSuspending {
            val (authorizer, api, timer) =
                scripted(DevicePoll.Pending, DevicePoll.Pending, DevicePoll.Approved(APPROVED))
            var shown: DeviceAuthChallenge? = null

            val token = authorizer.authorize(REQUEST) { shown = it }

            assertEquals(APPROVED, token)
            assertEquals(CHALLENGE, shown)
            assertEquals(listOf(5.seconds, 5.seconds, 5.seconds), timer.delays)
            assertEquals(3, api.polled)
        }

    @Test
    fun slowDownLengthensEveryLaterWaitByFiveSeconds() =
        runSuspending {
            val (authorizer, _, timer) =
                scripted(
                    DevicePoll.Pending,
                    DevicePoll.SlowDown,
                    DevicePoll.Pending,
                    DevicePoll.SlowDown,
                    DevicePoll.Approved(APPROVED),
                )

            authorizer.authorize(REQUEST) {}

            assertEquals(listOf(5, 5, 10, 10, 15).map { it.seconds }, timer.delays)
        }

    @Test
    fun anExpiredCodeEndsTheFlowWithTheTypedError() =
        runSuspending {
            val (authorizer, api, _) = scripted(DevicePoll.Pending, DevicePoll.Expired)

            assertFailsWith<RommException.DeviceCodeExpired> { authorizer.authorize(REQUEST) {} }

            assertEquals(2, api.polled)
        }

    @Test
    fun aDeniedDeviceEndsTheFlowWithTheTypedError() =
        runSuspending {
            val (authorizer, api, _) = scripted(DevicePoll.Pending, DevicePoll.Denied)

            assertFailsWith<RommException.DeviceAccessDenied> { authorizer.authorize(REQUEST) {} }

            assertEquals(2, api.polled)
        }

    @Test
    fun theFlowGivesUpWhenTheCodesLifetimeRunsOutEvenIfRommKeepsSayingPending() =
        runSuspending {
            val pending = Array(CHALLENGE.expiresIn) { DevicePoll.Pending }
            val (authorizer, api, timer) = scripted(*pending)

            assertFailsWith<RommException.DeviceCodeExpired> { authorizer.authorize(REQUEST) {} }

            assertEquals(CHALLENGE.expiresIn.seconds, timer.elapsed)
            assertEquals(CHALLENGE.expiresIn / CHALLENGE.interval - 1, api.polled)
        }

    @Test
    fun anOldServerIsRefusedBeforePairingStarts() =
        runSuspending {
            val (authorizer, api, timer) = scripted(version = "5.3.9")

            assertFailsWith<RommException.UnsupportedServer> { authorizer.authorize(REQUEST) {} }

            assertEquals(0, api.started)
            assertTrue(timer.delays.isEmpty())
        }

    @Test
    fun pairingChecksTheServerVersionThroughHttpToo() =
        runSuspending {
            val romm = MockRomm { json("""{"SYSTEM":{"VERSION":"5.4.0","SHOW_SETUP_WIZARD":false}}""") }

            assertFailsWith<RommException.UnsupportedServer> {
                DeviceAuthorizer(romm.deviceAuth, FakeTimer()).authorize(REQUEST) {}
            }

            assertEquals(listOf("/api/heartbeat"), romm.requests.map { it.url.encodedPath })
        }

    @Test
    fun theWholeFlowOverHttpEndsWithTheApprovedToken() =
        runSuspending {
            var polls = 0
            val romm =
                MockRomm { request ->
                    when (request.url.encodedPath) {
                        "/api/heartbeat" -> {
                            json("""{"SYSTEM":{"VERSION":"5.4.1","SHOW_SETUP_WIZARD":false}}""")
                        }

                        "/api/auth/device/init" -> {
                            json(
                                """{"device_code":"c","user_code":"U","verification_path":"/p",
                                   "verification_path_complete":"/p?u","expires_in":600,"interval":5}""",
                                HttpStatusCode.Created,
                            )
                        }

                        else -> {
                            if (polls++ == 0) {
                                failure(HttpStatusCode.BadRequest, "authorization_pending")
                            } else {
                                json(
                                    """{"access_token":"t","device_id":"d","scopes":[],"expires_at":null}""",
                                )
                            }
                        }
                    }
                }
            val timer = FakeTimer()

            val token = DeviceAuthorizer(romm.deviceAuth, timer).authorize(REQUEST) {}

            assertEquals("t", token.accessToken)
            assertEquals("d", token.deviceId)
            assertEquals(2, polls)
            assertEquals(listOf(5.seconds, 5.seconds), timer.delays)
            assertTrue(romm.requests.none { HttpHeaders.Authorization in it.headers })
            assertTrue(romm.tokens.rejected.isEmpty())
            assertEquals(4, romm.requests.size, "heartbeat, init and two polls")
        }
}
