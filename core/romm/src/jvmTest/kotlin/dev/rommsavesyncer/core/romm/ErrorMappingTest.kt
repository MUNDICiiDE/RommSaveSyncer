package dev.rommsavesyncer.core.romm

import io.ktor.client.engine.mock.respond
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import kotlinx.io.IOException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertTrue

class ErrorMappingTest {
    private fun failingWith(
        status: HttpStatusCode,
        detail: String = "nope",
    ) = MockRomm { failure(status, detail) }

    @Test
    fun aRequestCarriesTheBearerToken() =
        runSuspending {
            val romm = MockRomm { json("[]") }

            romm.client.listSaves()

            assertEquals("Bearer $TOKEN", romm.request.headers[HttpHeaders.Authorization])
        }

    @Test
    fun a401IsTypedAndTellsTheTokenSourceWhichTokenWasRejected() =
        runSuspending {
            val romm = failingWith(HttpStatusCode.Unauthorized, "Not authenticated")

            val error = assertFailsWith<RommException.Unauthorized> { romm.client.listSaves() }

            assertTrue("Not authenticated" in error.message.orEmpty())
            assertEquals(listOf<String?>(TOKEN), romm.tokens.rejected)
        }

    @Test
    fun withoutATokenNoRequestIsSentAndTheTokenSourceIsTold() =
        runSuspending {
            val romm = MockRomm(RecordingTokens(token = null)) { json("[]") }

            assertFailsWith<RommException.Unauthorized> { romm.client.listSaves() }

            assertTrue(romm.requests.isEmpty())
            assertEquals(listOf<String?>(null), romm.tokens.rejected)
        }

    @Test
    fun a403IsForbiddenAndDoesNotSignTheUserOut() =
        runSuspending {
            val romm = failingWith(HttpStatusCode.Forbidden, "Forbidden")

            assertFailsWith<RommException.Forbidden> { romm.client.listSaves() }

            assertTrue(romm.tokens.rejected.isEmpty())
        }

    @Test
    fun a404IsNotFound() =
        runSuspending {
            val romm = failingWith(HttpStatusCode.NotFound, "Save with ID 9 not found")

            val error = assertFailsWith<RommException.NotFound> { romm.client.downloadSave(9) }

            assertTrue("Save with ID 9 not found" in error.message.orEmpty())
        }

    @Test
    fun a409IsTheTypedConflictWithRommsExplanation() =
        runSuspending {
            val romm = failingWith(HttpStatusCode.Conflict, "Slot has a newer save since your last sync")

            val error = assertFailsWith<RommException.Conflict> { romm.client.listSaves() }

            assertEquals("Slot has a newer save since your last sync", error.detail)
        }

    @Test
    fun aDetailObjectUsesItsMessage() =
        runSuspending {
            val romm =
                MockRomm {
                    json(
                        """{"detail":{"error":"device_exists","message":"A device with this fingerprint exists"}}""",
                        HttpStatusCode.Conflict,
                    )
                }

            val error = assertFailsWith<RommException.Conflict> { romm.client.listSaves() }

            assertEquals("A device with this fingerprint exists", error.detail)
        }

    @Test
    fun validationFailuresAreInvalidRequests() =
        runSuspending {
            val body = """{"detail":[{"msg":"field required"}]}"""
            val romm = MockRomm { json(body, HttpStatusCode.UnprocessableEntity) }

            val error = assertFailsWith<RommException.InvalidRequest> { romm.client.listSaves() }

            assertEquals(HttpStatusCode.UnprocessableEntity.value, error.status)
            assertTrue("field required" in error.detail)
        }

    @Test
    fun a400IsAnInvalidRequest() =
        runSuspending {
            val error =
                assertFailsWith<RommException.InvalidRequest> {
                    failingWith(HttpStatusCode.BadRequest, "Sync is disabled for this device").client.listSaves()
                }

            assertEquals(HttpStatusCode.BadRequest.value, error.status)
        }

    @Test
    fun a429IsRateLimited() =
        runSuspending {
            assertFailsWith<RommException.RateLimited> {
                failingWith(HttpStatusCode.TooManyRequests, "Too many requests").client.listSaves()
            }
        }

    @Test
    fun serverFailuresKeepTheirStatus() =
        runSuspending {
            for (status in listOf(HttpStatusCode.InternalServerError, HttpStatusCode.ServiceUnavailable)) {
                val error = assertFailsWith<RommException.ServerError> { failingWith(status).client.listSaves() }
                assertEquals(status.value, error.status)
            }
        }

    @Test
    fun anUnexpectedStatusIsReportedAsSuch() =
        runSuspending {
            val error =
                assertFailsWith<RommException.UnexpectedResponse> {
                    failingWith(HttpStatusCode.PaymentRequired).client.listSaves()
                }

            assertEquals(HttpStatusCode.PaymentRequired.value, error.status)
        }

    @Test
    fun aNonJsonErrorBodyBecomesTheDetail() =
        runSuspending {
            val romm = MockRomm { respond("<html>Bad Gateway</html>", HttpStatusCode.BadGateway) }

            val error = assertFailsWith<RommException.ServerError> { romm.client.listSaves() }

            assertEquals("<html>Bad Gateway</html>", error.detail)
        }

    @Test
    fun aDroppedConnectionIsANetworkError() =
        runSuspending {
            val romm = MockRomm { throw IOException("connection reset") }

            val error = assertFailsWith<RommException.Network> { romm.client.listSaves() }

            assertTrue("connection reset" in error.message.orEmpty())
            assertIs<IOException>(error.cause)
            assertTrue(romm.tokens.rejected.isEmpty())
        }

    @Test
    fun aSuccessBodyOfTheWrongShapeIsAnUnexpectedResponse() =
        runSuspending {
            val romm = MockRomm { json("""{"not":"a list"}""") }

            val error = assertFailsWith<RommException.UnexpectedResponse> { romm.client.listSaves() }

            assertEquals(HttpStatusCode.OK.value, error.status)
            assertTrue("malformed body" in error.detail)
        }
}
