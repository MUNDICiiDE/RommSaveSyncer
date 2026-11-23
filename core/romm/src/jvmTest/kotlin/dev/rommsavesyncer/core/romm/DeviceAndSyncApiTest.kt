package dev.rommsavesyncer.core.romm

import io.ktor.http.ContentType
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlin.time.Instant

class DeviceAndSyncApiTest {
    private val deviceCreated = """{"device_id":"dev-1","name":"Windows PC","created_at":"2026-11-23T10:00:00+00:00"}"""

    @Test
    fun registeringADeviceSendsTheFingerprintFieldsAsJson() =
        runSuspending {
            val romm = MockRomm { json(deviceCreated, HttpStatusCode.Created) }

            romm.client.registerDevice(
                DeviceRegistration(
                    name = "Windows PC",
                    platform = "windows",
                    client = "RommSaveSyncer",
                    clientVersion = "0.1.0",
                    macAddress = "aa:bb:cc:dd:ee:ff",
                    hostname = "pc",
                    syncMode = SyncMode.API,
                    capabilities = mapOf("remote_install" to false),
                ),
            )

            assertEquals(HttpMethod.Post, romm.request.method)
            assertEquals("/api/devices", romm.request.url.encodedPath)
            assertEquals(
                ContentType.Application.Json,
                romm.request.body.contentType
                    ?.withoutParameters(),
            )
            val body = romm.request.bodyJson()
            assertEquals("Windows PC", body.field("name")?.jsonPrimitive?.content)
            assertEquals("windows", body.field("platform")?.jsonPrimitive?.content)
            assertEquals("RommSaveSyncer", body.field("client")?.jsonPrimitive?.content)
            assertEquals("0.1.0", body.field("client_version")?.jsonPrimitive?.content)
            assertEquals("aa:bb:cc:dd:ee:ff", body.field("mac_address")?.jsonPrimitive?.content)
            assertEquals("pc", body.field("hostname")?.jsonPrimitive?.content)
            assertEquals("api", body.field("sync_mode")?.jsonPrimitive?.content)
            val capabilities = body.getValue("capabilities").jsonObject
            assertFalse(capabilities.getValue("remote_install").jsonPrimitive.boolean)
            assertTrue(body.getValue("allow_existing").jsonPrimitive.boolean)
            assertFalse(body.getValue("allow_duplicate").jsonPrimitive.boolean)
        }

    @Test
    fun a201MeansTheDeviceIsNewAndA200MeansItWasAlreadyRegistered() =
        runSuspending {
            val newDevice = MockRomm { json(deviceCreated, HttpStatusCode.Created) }.client
            val knownDevice = MockRomm { json(deviceCreated, HttpStatusCode.OK) }.client

            val created = newDevice.registerDevice(DeviceRegistration())
            val existing = knownDevice.registerDevice(DeviceRegistration())

            assertFalse(created.alreadyExisted)
            assertTrue(existing.alreadyExisted)
            assertEquals("dev-1", existing.deviceId)
            assertEquals(Instant.parse("2026-11-23T10:00:00Z"), existing.createdAt)
        }

    @Test
    fun negotiatingSendsTheSavesAndOmitsANullSlot() =
        runSuspending {
            val romm = MockRomm { json(NEGOTIATE_RESPONSE) }
            val updated = Instant.parse("2026-11-23T09:30:00Z")

            romm.client.negotiate(
                NegotiateRequest(
                    deviceId = "dev-1",
                    saves =
                        listOf(
                            ClientSaveState(3, "Game.srm", "autosave", "retroarch", "abc", updated, 32768),
                            ClientSaveState(3, "Game.srm", null, null, "abc", updated, 32768),
                        ),
                    romIds = listOf(3),
                ),
            )

            assertEquals("/api/sync/negotiate", romm.request.url.encodedPath)
            val body = romm.request.bodyJson()
            assertEquals("dev-1", body.field("device_id")?.jsonPrimitive?.content)
            assertEquals(listOf(3), body.getValue("rom_ids").jsonArray.map { it.jsonPrimitive.int })
            assertFalse(body.getValue("restore_unlisted").jsonPrimitive.boolean)
            val saves = body.getValue("saves").jsonArray.map { it.jsonObject }
            assertEquals("autosave", saves[0].getValue("slot").jsonPrimitive.content)
            assertEquals("retroarch", saves[0].getValue("emulator").jsonPrimitive.content)
            assertEquals("abc", saves[0].getValue("content_hash").jsonPrimitive.content)
            assertEquals("2026-11-23T09:30:00Z", saves[0].getValue("updated_at").jsonPrimitive.content)
            assertEquals(32768, saves[0].getValue("file_size_bytes").jsonPrimitive.int)
            assertEquals(3, saves[0].getValue("rom_id").jsonPrimitive.int)
            assertEquals("Game.srm", saves[0].getValue("file_name").jsonPrimitive.content)
            assertFalse("slot" in saves[1], "an archival save is sent without a slot, as RomM's null")
            assertFalse(saves[1].values.any { it is JsonNull })
        }

    @Test
    fun theNegotiationResponseListsEveryKindOfOperation() =
        runSuspending {
            val response = MockRomm { json(NEGOTIATE_RESPONSE) }.client.negotiate(NegotiateRequest(saves = emptyList()))

            assertEquals(12, response.sessionId)
            assertEquals(
                listOf(
                    SyncAction.UPLOAD,
                    SyncAction.DOWNLOAD,
                    SyncAction.CONFLICT,
                    SyncAction.NO_OP,
                    SyncAction.DELETE,
                ),
                response.operations.map { it.action },
            )
            val download = response.operations[1]
            assertEquals(7, download.saveId)
            assertEquals("autosave", download.slot)
            assertEquals("abc123", download.serverContentHash)
            assertEquals(Instant.parse("2026-11-23T10:00:00Z"), download.serverUpdatedAt)
            assertNull(response.operations[0].saveId)
            assertEquals(1, response.totalUpload)
            assertEquals(1, response.totalDelete)
        }

    @Test
    fun completingASessionReportsPlaySessionsAndReadsTheIngestResult() =
        runSuspending {
            val romm = MockRomm { json(COMPLETE_RESPONSE) }
            val start = Instant.parse("2026-11-23T10:00:00Z")
            val end = Instant.parse("2026-11-23T10:30:00Z")

            val response =
                romm.client.completeSession(
                    12,
                    CompleteSessionRequest(
                        operationsCompleted = 2,
                        operationsFailed = 1,
                        playSessions = listOf(PlaySessionEntry(3, "autosave", start, end, 1_800_000)),
                    ),
                )

            assertEquals("/api/sync/sessions/12/complete", romm.request.url.encodedPath)
            val body = romm.request.bodyJson()
            assertEquals(2, body.getValue("operations_completed").jsonPrimitive.int)
            assertEquals(1, body.getValue("operations_failed").jsonPrimitive.int)
            val play =
                body
                    .getValue("play_sessions")
                    .jsonArray
                    .single()
                    .jsonObject
            assertEquals(3, play.getValue("rom_id").jsonPrimitive.int)
            assertEquals("autosave", play.getValue("save_slot").jsonPrimitive.content)
            assertEquals("2026-11-23T10:00:00Z", play.getValue("start_time").jsonPrimitive.content)
            assertEquals("2026-11-23T10:30:00Z", play.getValue("end_time").jsonPrimitive.content)
            assertEquals(1_800_000, play.getValue("duration_ms").jsonPrimitive.int)
            assertEquals(SyncSessionStatus.COMPLETED, response.session.status)
            assertEquals(2, response.session.operationsCompleted)
            val ingest = assertNotNull(response.playSessionIngest)
            assertEquals(1, ingest.createdCount)
            assertEquals(PlaySessionIngestStatus.CREATED, ingest.results.single().status)
        }

    @Test
    fun completingWithoutPlaySessionsOmitsThemAndToleratesNoIngestResult() =
        runSuspending {
            val romm = MockRomm { json(COMPLETE_WITHOUT_INGEST) }

            val response = romm.client.completeSession(12)

            assertFalse("play_sessions" in romm.request.bodyJson())
            assertNull(response.playSessionIngest)
        }

    private companion object {
        val NEGOTIATE_RESPONSE =
            """
            {"session_id":12,"total_upload":1,"total_download":1,"total_conflict":1,"total_no_op":1,"total_delete":1,
             "operations":[
              {"action":"upload","rom_id":3,"save_id":null,"file_name":"Game.srm","slot":"autosave","emulator":null,
               "reason":"Save exists on client but not on server","server_updated_at":null,"server_content_hash":null},
              {"action":"download","rom_id":4,"save_id":7,"file_name":"Other.srm","slot":"autosave","emulator":"retroarch",
               "reason":"Server save is newer","server_updated_at":"2026-11-23T10:00:00+00:00",
               "server_content_hash":"abc123"},
              {"action":"conflict","rom_id":5,"save_id":8,"file_name":"A.srm","slot":"autosave","emulator":null,
               "reason":"Both sides changed since last sync"},
              {"action":"no_op","rom_id":6,"save_id":9,"file_name":"B.srm","slot":"autosave","emulator":null,
               "reason":"Content is identical"},
              {"action":"delete","rom_id":2,"save_id":null,"file_name":"C.srm","slot":"autosave","emulator":null,
               "reason":"Save was deleted on the server"}]}
            """.trimIndent()

        const val SESSION =
            """"session":{"id":12,"device_id":"dev-1","user_id":1,"status":"COMPLETED","initiated_at":"2026-11-23T10:00:00+00:00",
               "completed_at":"2026-11-23T10:31:00+00:00","operations_planned":3,"operations_completed":2,
               "operations_failed":1,"error_message":null,"created_at":"2026-11-23T10:00:00+00:00",
               "updated_at":"2026-11-23T10:31:00+00:00"}"""

        val COMPLETE_RESPONSE =
            """{$SESSION,"play_session_ingest":{"results":[{"index":0,"status":"created","id":5,"detail":null}],
               "created_count":1,"skipped_count":0}}"""

        val COMPLETE_WITHOUT_INGEST = """{$SESSION,"play_session_ingest":null}"""
    }
}
