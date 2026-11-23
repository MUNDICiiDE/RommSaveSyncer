package dev.rommsavesyncer.core.romm

import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.toByteArray
import io.ktor.client.request.HttpRequestData
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlinx.serialization.json.jsonPrimitive
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlin.time.Instant

class SavesApiTest {
    private val save = ByteArray(256) { it.toByte() }

    private fun upload(
        slot: String? = "autosave",
        configure: (SaveUpload) -> SaveUpload = { it },
    ) = configure(SaveUpload(romId = 3, file = SaveFile("Game.srm", save), slot = slot))

    private suspend fun HttpRequestData.multipartText(): String = String(body.toByteArray(), Charsets.ISO_8859_1)

    @Test
    fun anUploadIsAMultipartPostWithOnlyTheSaveFilePart() =
        runSuspending {
            val romm = MockRomm { json(SAVE_JSON, HttpStatusCode.OK) }

            romm.client.uploadSave(upload())

            assertEquals(HttpMethod.Post, romm.request.method)
            assertEquals("/api/saves", romm.request.url.encodedPath)
            val contentType = checkNotNull(romm.request.body.contentType)
            assertEquals(ContentType.MultiPart.FormData, contentType.withoutParameters())
            val text = romm.request.multipartText()
            val boundary = Regex.escape("--${checkNotNull(contentType.parameter("boundary"))}")
            assertEquals(2, Regex(boundary).findAll(text).count(), "one part, opened and closed")
            assertTrue(
                Regex(
                    """Content-Disposition: form-data; name="?saveFile"?; filename="Game\.srm"""",
                ).containsMatchIn(text),
                text,
            )
            assertTrue("Content-Type: application/octet-stream" in text)
            assertTrue(String(save, Charsets.ISO_8859_1) in text, "the bytes travel unchanged")
            assertEquals(1, Regex("Content-Disposition").findAll(text).count())
        }

    @Test
    fun aFileNameCannotBreakOutOfTheContentDispositionHeader() =
        runSuspending {
            val romm = MockRomm { json(SAVE_JSON, HttpStatusCode.OK) }
            val name = "a\r\nb\\c\"d.srm"

            romm.client.uploadSave(upload { it.copy(file = SaveFile(name, save)) })

            val text = romm.request.multipartText()
            assertTrue("""filename="a  b\\c%22d.srm"""" in text, text)
            assertEquals(1, Regex("Content-Disposition").findAll(text).count())
            assertTrue("\r\nb" !in text, "the name's line break must not start a new header line")
        }

    @Test
    fun theUploadMetadataTravelsAsQueryParametersNotFormFields() =
        runSuspending {
            val romm = MockRomm { json(SAVE_JSON) }

            romm.client.uploadSave(
                upload {
                    it.copy(
                        emulator = "retroarch",
                        deviceId = "dev-1",
                        sessionId = 12,
                        contentHash = "0123456789abcdef0123456789abcdef",
                        overwrite = true,
                        autocleanupLimit = 5,
                    )
                },
            )

            val request = romm.request
            assertEquals("3", request.query("rom_id"))
            assertEquals("autosave", request.query("slot"))
            assertEquals("retroarch", request.query("emulator"))
            assertEquals("dev-1", request.query("device_id"))
            assertEquals("12", request.query("session_id"))
            assertEquals("0123456789abcdef0123456789abcdef", request.query("content_hash"))
            assertEquals("true", request.query("overwrite"))
            assertEquals("true", request.query("autocleanup"))
            assertEquals("5", request.query("autocleanup_limit"))
            val text = request.multipartText()
            for (field in listOf("rom_id", "slot", "device_id", "content_hash", "overwrite")) {
                assertTrue("name=$field" !in text && "name=\"$field\"" !in text, "$field must not be a form field")
            }
        }

    @Test
    fun aNullSlotUploadSendsNoSlotAtAllAndDoesNotOverwriteByDefault() =
        runSuspending {
            val romm = MockRomm { json(SAVE_JSON.replace("\"slot\":\"autosave\"", "\"slot\":null")) }

            val stored = romm.client.uploadSave(upload(slot = null))

            assertNull(romm.request.query("slot"))
            assertEquals("false", romm.request.query("overwrite"))
            assertNull(romm.request.query("autocleanup"))
            assertNull(stored.slot)
        }

    @Test
    fun theUploadResponseIsParsedIntoASave() =
        runSuspending {
            val stored = MockRomm { json(SAVE_JSON) }.client.uploadSave(upload())

            assertEquals(7, stored.id)
            assertEquals(3, stored.romId)
            assertEquals("autosave", stored.slot)
            assertEquals("abc123", stored.contentHash)
            assertEquals("dev-1", stored.originDeviceId)
            assertEquals(Instant.parse("2026-11-23T10:00:00.5Z"), stored.updatedAt)
            val sync = stored.deviceSyncs.single()
            assertEquals("dev-1", sync.deviceId)
            assertTrue(sync.isCurrent)
        }

    @Test
    fun aStaleUploadIsRefusedWithTheTypedConflict() =
        runSuspending {
            val romm = MockRomm { failure(HttpStatusCode.Conflict, "Slot has a newer save since your last sync") }

            val refused = assertFailsWith<RommException.Conflict> { romm.client.uploadSave(upload()) }

            assertEquals("Slot has a newer save since your last sync", refused.detail)
            assertTrue(romm.tokens.rejected.isEmpty(), "a 409 is not a sign-in problem")
        }

    @Test
    fun downloadingReturnsTheBytesAndPassesTheDeviceContext() =
        runSuspending {
            val romm =
                MockRomm {
                    respond(save, headers = headersOf(HttpHeaders.ContentType, "application/octet-stream"))
                }

            val bytes = romm.client.downloadSave(7, deviceId = "dev-1", sessionId = 12, optimistic = false)

            assertContentEquals(save, bytes)
            assertEquals(HttpMethod.Get, romm.request.method)
            assertEquals("/api/saves/7/content", romm.request.url.encodedPath)
            assertEquals("dev-1", romm.request.query("device_id"))
            assertEquals("12", romm.request.query("session_id"))
            assertEquals("false", romm.request.query("optimistic"))
        }

    @Test
    fun downloadingWithoutADeviceSendsOnlyTheOptimisticFlag() =
        runSuspending {
            val romm = MockRomm { respond(save) }

            romm.client.downloadSave(7)

            assertNull(romm.request.query("device_id"))
            assertNull(romm.request.query("session_id"))
            assertEquals("true", romm.request.query("optimistic"))
        }

    @Test
    fun confirmingADownloadPostsTheDeviceAndHash() =
        runSuspending {
            val romm = MockRomm { json(SAVE_JSON) }

            val confirmed = romm.client.confirmDownload(7, "dev-1", "abc123")

            assertEquals(HttpMethod.Post, romm.request.method)
            assertEquals("/api/saves/7/downloaded", romm.request.url.encodedPath)
            val body = romm.request.bodyJson()
            assertEquals("dev-1", body.getValue("device_id").jsonPrimitive.content)
            assertEquals("abc123", body.getValue("content_hash").jsonPrimitive.content)
            assertEquals(7, confirmed.id)
        }

    @Test
    fun listingSavesFiltersByRomSlotAndDevice() =
        runSuspending {
            val romm = MockRomm { json("[$SAVE_JSON]") }

            val saves = romm.client.listSaves(romId = 3, slot = "autosave", deviceId = "dev-1")

            assertEquals(listOf(7), saves.map { it.id })
            assertEquals("/api/saves", romm.request.url.encodedPath)
            assertEquals("3", romm.request.query("rom_id"))
            assertEquals("autosave", romm.request.query("slot"))
            assertEquals("dev-1", romm.request.query("device_id"))
        }
}
