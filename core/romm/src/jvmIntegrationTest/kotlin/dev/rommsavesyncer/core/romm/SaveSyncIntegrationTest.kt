package dev.rommsavesyncer.core.romm

import dev.rommsavesyncer.testing.fixtures.FixtureSystem
import dev.rommsavesyncer.testing.harness.RommEnvironment
import dev.rommsavesyncer.testing.harness.RommServer
import kotlinx.coroutines.runBlocking
import org.junit.jupiter.api.Assertions.assertArrayEquals
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeAll
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.TestInstance
import org.junit.jupiter.api.assertThrows
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Instant

/**
 * Two simulated Devices against RomM 5.4.1. The server is shared by every test, so each test uses its own
 * fixture ROM and keeps its saves apart from the others'.
 */
@RommServer
@TestInstance(TestInstance.Lifecycle.PER_CLASS)
class SaveSyncIntegrationTest {
    private lateinit var env: RommEnvironment
    private lateinit var deviceA: SimulatedDevice
    private lateinit var deviceB: SimulatedDevice

    @BeforeAll
    fun pairDevices(environment: RommEnvironment) {
        env = environment
        TwoDevices.of(env).let {
            deviceA = it.a
            deviceB = it.b
        }
    }

    private fun romId(system: FixtureSystem): Int = env.romId(fixtureRom(system))

    @Test
    fun authNegotiateUploadDownloadConfirmCompleteWithAPlaySession() =
        runBlocking<Unit> {
            val romId = romId(FixtureSystem.GBA)
            val bytes = saveBytes("happy path")
            val end = wholeSecondNow()
            val start = end - 30.minutes

            // Device A has a save RomM lacks: negotiate says upload, and the upload stores the MD5 we sent.
            val uploadPlan =
                deviceA.client.negotiate(
                    NegotiateRequest(saves = listOf(clientState(romId, bytes, start))),
                )
            assertEquals(
                listOf(SyncAction.UPLOAD),
                uploadPlan.operations.filter { it.romId == romId }.map { it.action },
            )
            val stored = deviceA.client.uploadSave(saveUpload(deviceA, romId, bytes, sessionId = uploadPlan.sessionId))
            assertEquals(md5(bytes), stored.contentHash)
            assertEquals(SLOT, stored.slot)
            assertEquals(romId, stored.romId)
            assertEquals(bytes.size.toLong(), stored.fileSizeBytes)
            assertEquals(deviceA.deviceId, stored.originDeviceId)
            deviceA.client.completeSession(uploadPlan.sessionId, CompleteSessionRequest(operationsCompleted = 1))

            // Device B has nothing: negotiate offers the save, B downloads and confirms it.
            val downloadPlan = deviceB.client.negotiate(NegotiateRequest(saves = emptyList(), romIds = listOf(romId)))
            val operation = downloadPlan.operations.single { it.romId == romId }
            assertEquals(SyncAction.DOWNLOAD, operation.action)
            assertEquals(stored.id, operation.saveId)
            assertEquals(md5(bytes), operation.serverContentHash)
            val downloaded = deviceB.client.downloadSave(stored.id, deviceB.deviceId, downloadPlan.sessionId)
            assertArrayEquals(bytes, downloaded)
            val confirmed = deviceB.client.confirmDownload(stored.id, deviceB.deviceId, md5(downloaded))
            assertTrue(confirmed.deviceSyncs.single { it.deviceId == deviceB.deviceId }.isCurrent)

            // B ends the session with the play session it just had.
            val playSession = PlaySessionEntry(romId, SLOT, start, end, (end - start).inWholeMilliseconds)
            val completed =
                deviceB.client.completeSession(
                    downloadPlan.sessionId,
                    CompleteSessionRequest(operationsCompleted = 1, playSessions = listOf(playSession)),
                )
            assertEquals(SyncSessionStatus.COMPLETED, completed.session.status)
            assertEquals(1, completed.session.operationsCompleted)
            assertEquals(1, checkNotNull(completed.playSessionIngest).createdCount)
            val recorded = deviceB.client.listPlaySessions(deviceId = deviceB.deviceId, romId = romId).single()
            assertEquals(SLOT, recorded.saveSlot)
            assertEquals(deviceB.deviceId, recorded.deviceId)
            assertEquals(start, recorded.startTime)
            assertEquals(end, recorded.endTime)
            assertEquals((end - start).inWholeMilliseconds, recorded.durationMs)

            // Both devices now hold the same bytes, so a later negotiation changes nothing.
            val later = deviceB.client.negotiate(NegotiateRequest(saves = listOf(clientState(romId, bytes, end))))
            assertEquals(SyncAction.NO_OP, later.operations.single { it.romId == romId }.action)
            deviceB.client.completeSession(later.sessionId)
        }

    @Test
    fun aStaleUploadIsRefusedWithTheTypedConflictUntilItIsOverwritten() =
        runBlocking<Unit> {
            val romId = romId(FixtureSystem.GB)
            val first = saveBytes("A's save")
            val second = saveBytes("B's save")
            val third = saveBytes("A's later save")
            deviceA.client.uploadSave(saveUpload(deviceA, romId, first))

            val refusedB =
                assertThrows<RommException.Conflict> { deviceB.client.uploadSave(saveUpload(deviceB, romId, second)) }
            assertTrue("newer save" in refusedB.detail, refusedB.detail)

            awaitNextServerSecond()
            val forced = deviceB.client.uploadSave(saveUpload(deviceB, romId, second, overwrite = true))
            assertEquals(md5(second), forced.contentHash)
            assertThrows<RommException.Conflict> { deviceA.client.uploadSave(saveUpload(deviceA, romId, third)) }

            val slotHashes = deviceA.client.listSaves(romId = romId, slot = SLOT).map { it.contentHash }
            assertTrue(md5(third) !in slotHashes, "the refused upload stored nothing")
            assertTrue(md5(second) in slotHashes)
            assertTrue(deviceA.tokens.rejected.isEmpty() && deviceB.tokens.rejected.isEmpty(), "409 is not a 401")
        }

    @Test
    fun aNullSlotUploadIsArchivalAndNeverPairedWithTheSlottedSave() =
        runBlocking<Unit> {
            val romId = romId(FixtureSystem.GBC)
            val bytes = saveBytes("archive")

            val archived = deviceA.client.uploadSave(saveUpload(deviceA, romId, bytes, slot = null))

            assertNull(archived.slot)
            assertEquals(md5(bytes), archived.contentHash)
            assertEquals(listOf(archived.id), deviceA.client.listSaves(romId = romId).map { it.id })
            assertEquals(emptyList<RommSave>(), deviceA.client.listSaves(romId = romId, slot = SLOT))
            val state = clientState(romId, bytes, wholeSecondNow(), slot = null)
            val plan = deviceA.client.negotiate(NegotiateRequest(saves = listOf(state)))
            val operation = plan.operations.single { it.romId == romId }
            assertEquals(SyncAction.UPLOAD, operation.action, "an identical archive still negotiates as an upload")
            assertNull(operation.saveId)
            deviceA.client.completeSession(plan.sessionId)
        }

    /**
     * Pins RomM 5.4.1, which does NOT report a conflict here (PLAN section 9, R1). Negotiation pairs a client save
     * with only the newest row of its slot, and every slotted upload creates a new row, so B has no sync history
     * for A's newer row. Without history RomM compares timestamps alone ("no sync history"), and B's later
     * timestamp reads as a plain upload. The sync engine must decide conflicts from its own baselines.
     */
    @Test
    fun whenBothDevicesChangedTheirSaveRomMStillNegotiatesAnUpload() =
        runBlocking<Unit> {
            val romId = romId(FixtureSystem.NES)
            val original = saveBytes("shared v1")
            val fromA = saveBytes("A v2")
            val fromB = saveBytes("B v3")
            val stored = deviceA.client.uploadSave(saveUpload(deviceA, romId, original))
            deviceB.client.downloadSave(stored.id, deviceB.deviceId)
            deviceB.client.confirmDownload(stored.id, deviceB.deviceId, md5(original))
            awaitNextServerSecond()
            val newest = deviceA.client.uploadSave(saveUpload(deviceA, romId, fromA))

            val localChange = clientState(romId, fromB, wholeSecondNow() + 1.minutes)
            val plan = deviceB.client.negotiate(NegotiateRequest(saves = listOf(localChange)))

            val operation = plan.operations.single { it.romId == romId }
            assertEquals(SyncAction.UPLOAD, operation.action)
            assertEquals(newest.id, operation.saveId)
            assertEquals(md5(fromA), operation.serverContentHash)
            assertEquals("Client save is newer (no sync history)", operation.reason)
            deviceB.client.completeSession(plan.sessionId)
        }

    /**
     * Pins a RomM 5.4.1 quirk: the upload response is built from the in-memory row, which keeps the microseconds of
     * `updated_at`, while the stored row (MariaDB TIMESTAMP without precision) holds whole seconds. Sync baselines
     * (M4+) must take the server timestamp from a re-read or negotiate, never from the upload response.
     */
    @Test
    fun theStoredUpdatedAtIsTheUploadResponseTruncatedToWholeSeconds() =
        runBlocking<Unit> {
            val romId = romId(FixtureSystem.NDS)
            val uploaded = deviceA.client.uploadSave(saveUpload(deviceA, romId, saveBytes("A on nds")))

            val reread = deviceA.client.listSaves(romId = romId, slot = SLOT).single { it.id == uploaded.id }

            assertEquals(0, reread.updatedAt.nanosecondsOfSecond, "stored updatedAt has no sub-second part")
            assertEquals(
                Instant.fromEpochSeconds(uploaded.updatedAt.epochSeconds),
                reread.updatedAt,
                "stored updatedAt is the upload response's updatedAt truncated to whole seconds",
            )
        }

    /** The conflict RomM 5.4.1 really reports: no sync history, equal timestamps, different content. */
    @Test
    fun aSaveWithTheServersTimestampButOtherContentNegotiatesAsAConflict() =
        runBlocking<Unit> {
            val romId = romId(FixtureSystem.SNES)
            val fromA = saveBytes("A on snes")
            val fromB = saveBytes("B on snes")
            val uploaded = deviceA.client.uploadSave(saveUpload(deviceA, romId, fromA))
            // The upload response keeps microseconds the stored row lacks, so take the timestamp from a re-read.
            val stored = deviceA.client.listSaves(romId = romId, slot = SLOT).single { it.id == uploaded.id }

            val sameTime = clientState(romId, fromB, stored.updatedAt)
            val plan = deviceB.client.negotiate(NegotiateRequest(saves = listOf(sameTime)))

            val operation = plan.operations.single { it.romId == romId }
            assertEquals(SyncAction.CONFLICT, operation.action)
            assertEquals(stored.id, operation.saveId)
            assertEquals(md5(fromA), operation.serverContentHash)
            assertEquals("Same timestamp but different content", operation.reason)
            deviceB.client.completeSession(plan.sessionId, CompleteSessionRequest(operationsFailed = 1))
        }
}
