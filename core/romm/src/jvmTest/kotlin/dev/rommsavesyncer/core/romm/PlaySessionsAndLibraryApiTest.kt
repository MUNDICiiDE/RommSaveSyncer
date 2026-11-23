package dev.rommsavesyncer.core.romm

import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull
import kotlin.time.Instant

class PlaySessionsAndLibraryApiTest {
    private val start = Instant.parse("2026-11-23T10:00:00Z")
    private val end = Instant.parse("2026-11-23T10:30:00Z")

    @Test
    fun ingestingPlaySessionsPostsTheBatchAndReportsEachResult() =
        runSuspending {
            val romm =
                MockRomm {
                    json(
                        """{"results":[{"index":0,"status":"created","id":5},{"index":1,"status":"duplicate"},
                           {"index":2,"status":"error","detail":"end_time is too far in the future"}],
                           "created_count":1,"skipped_count":2}""",
                        HttpStatusCode.Created,
                    )
                }

            val result =
                romm.client.ingestPlaySessions(
                    listOf(
                        PlaySessionEntry(3, "autosave", start, end, 1_800_000),
                        PlaySessionEntry(null, null, start, end, 1_800_000),
                        PlaySessionEntry(3, "autosave", start, end, 1_800_000),
                    ),
                    deviceId = "dev-1",
                )

            assertEquals(HttpMethod.Post, romm.request.method)
            assertEquals("/api/play-sessions", romm.request.url.encodedPath)
            val body = romm.request.bodyJson()
            assertEquals("dev-1", body.getValue("device_id").jsonPrimitive.content)
            val sessions = body.getValue("sessions").jsonArray.map { it.jsonObject }
            assertEquals(3, sessions.size)
            assertEquals(3, sessions[0].getValue("rom_id").jsonPrimitive.int)
            assertEquals("2026-11-23T10:30:00Z", sessions[0].getValue("end_time").jsonPrimitive.content)
            assertNull(sessions[1]["rom_id"])
            assertEquals(
                listOf(
                    PlaySessionIngestStatus.CREATED,
                    PlaySessionIngestStatus.DUPLICATE,
                    PlaySessionIngestStatus.ERROR,
                ),
                result.results.map { it.status },
            )
            assertEquals(5, result.results[0].id)
            assertEquals("end_time is too far in the future", result.results[2].detail)
            assertEquals(1, result.createdCount)
            assertEquals(2, result.skippedCount)
        }

    @Test
    fun listingPlaySessionsFiltersByDeviceAndRomAndReadsTimestampsWithoutAnOffset() =
        runSuspending {
            val romm =
                MockRomm {
                    json(
                        """[{"id":5,"user_id":1,"device_id":"dev-1","rom_id":3,"save_slot":"autosave",
                           "start_time":"2026-11-23T10:00:00","end_time":"2026-11-23T10:30:00","duration_ms":1800000,
                           "created_at":"2026-11-23T10:31:00","updated_at":"2026-11-23T10:31:00"}]""",
                    )
                }

            val sessions = romm.client.listPlaySessions(deviceId = "dev-1", romId = 3)

            assertEquals("/api/play-sessions", romm.request.url.encodedPath)
            assertEquals("dev-1", romm.request.query("device_id"))
            assertEquals("3", romm.request.query("rom_id"))
            val session = sessions.single()
            assertEquals(start, session.startTime)
            assertEquals(end, session.endTime)
            assertEquals(1_800_000, session.durationMs)
            assertEquals("autosave", session.saveSlot)
        }

    @Test
    fun listingRomsSendsTheSearchAndPlatformAndSkipsTheWebUiIndexes() =
        runSuspending {
            val romm = MockRomm { json(ROM_PAGE) }

            val page = romm.client.listRoms(searchTerm = "mario", platformId = 4, limit = 10, offset = 20)

            assertEquals("/api/roms", romm.request.url.encodedPath)
            assertEquals("mario", romm.request.query("search_term"))
            assertEquals("4", romm.request.query("platform_ids"))
            assertEquals("10", romm.request.query("limit"))
            assertEquals("20", romm.request.query("offset"))
            assertEquals("false", romm.request.query("with_char_index"))
            assertEquals("false", romm.request.query("with_filter_values"))
            assertEquals("false", romm.request.query("with_rom_id_index"))
            assertEquals(1, page.total)
            val rom = page.items.single()
            assertEquals(3, rom.id)
            assertEquals("Game.gba", rom.fileName)
            assertEquals("gba", rom.platformSlug)
            assertEquals("d41d8cd98f00b204e9800998ecf8427e", rom.md5)
        }

    @Test
    fun findingARomByHashSendsOnlyTheGivenHashes() =
        runSuspending {
            val romm = MockRomm { json(ROM) }

            val rom = romm.client.findRomByHash(md5 = "d41d8cd98f00b204e9800998ecf8427e")

            assertEquals(3, rom?.id)
            assertEquals("/api/roms/by-hash", romm.request.url.encodedPath)
            assertEquals("d41d8cd98f00b204e9800998ecf8427e", romm.request.query("md5_hash"))
            assertNull(romm.request.query("sha1_hash"))
            assertNull(romm.request.query("crc_hash"))
        }

    @Test
    fun aHashRommDoesNotKnowIsNullNotAnError() =
        runSuspending {
            val romm = MockRomm { failure(HttpStatusCode.NotFound, "No ROM or file found with given hash values") }

            assertNull(romm.client.findRomByHash(sha1 = "0".repeat(40)))
        }

    @Test
    fun aHashLookupWithoutAnyHashIsRejectedBeforeCallingRomm() =
        runSuspending {
            val romm = MockRomm { json(ROM) }

            assertFailsWith<IllegalArgumentException> { romm.client.findRomByHash() }

            assertEquals(0, romm.requests.size)
        }

    private companion object {
        const val ROM =
            """{"id":3,"name":"Game","fs_name":"Game.gba","fs_size_bytes":32768,"platform_id":4,"platform_slug":"gba",
               "md5_hash":"d41d8cd98f00b204e9800998ecf8427e","sha1_hash":null,"crc_hash":"00000000","igdb_id":null}"""

        const val ROM_PAGE = """{"items":[$ROM],"total":1,"limit":10,"offset":20,"char_index":{}}"""
    }
}
