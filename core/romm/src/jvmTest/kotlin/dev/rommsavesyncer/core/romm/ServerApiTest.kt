package dev.rommsavesyncer.core.romm

import io.ktor.http.HttpHeaders
import io.ktor.http.HttpMethod
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNull

class ServerApiTest {
    private fun heartbeat(version: String) =
        MockRomm {
            json("""{"SYSTEM":{"VERSION":"$version","GIT_BRANCH":null,"SHOW_SETUP_WIZARD":false},"METADATA":{}}""")
        }.client

    @Test
    fun heartbeatReportsTheVersionWithoutCredentials() =
        runSuspending {
            val romm =
                MockRomm { json("""{"SYSTEM":{"VERSION":"5.4.1","SHOW_SETUP_WIZARD":false}}""") }

            val info = romm.client.serverInfo()

            assertEquals("5.4.1", info.version)
            assertFalse(info.showSetupWizard)
            assertEquals(HttpMethod.Get, romm.request.method)
            assertEquals("/api/heartbeat", romm.request.url.encodedPath)
            assertNull(romm.request.headers[HttpHeaders.Authorization])
        }

    @Test
    fun theMinimumVersionAndNewerAreAccepted() =
        runSuspending {
            assertEquals("5.4.1", heartbeat("5.4.1").requireSupportedServer().version)
            assertEquals("5.5.0-beta.1", heartbeat("5.5.0-beta.1").requireSupportedServer().version)
        }

    @Test
    fun anOlderServerIsRefusedWithItsVersion() =
        runSuspending {
            val client = heartbeat("5.4.0")

            val refused = assertFailsWith<RommException.UnsupportedServer> { client.requireSupportedServer() }

            assertEquals("5.4.0", refused.reportedVersion)
        }

    @Test
    fun aServerThatReportsNoUsableVersionIsRefused() =
        runSuspending {
            val client = heartbeat("<unknown>")

            assertFailsWith<RommException.UnsupportedServer> { client.requireSupportedServer() }
        }
}
