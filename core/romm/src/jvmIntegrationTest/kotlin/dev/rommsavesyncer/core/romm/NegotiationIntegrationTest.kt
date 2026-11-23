package dev.rommsavesyncer.core.romm

import dev.rommsavesyncer.testing.fixtures.FixtureSystem
import dev.rommsavesyncer.testing.harness.RommEnvironment
import dev.rommsavesyncer.testing.harness.RommServer
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.BeforeAll
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.TestInstance
import kotlin.time.Duration.Companion.hours

/**
 * What RomM 5.4.1's negotiation answers in the cases the sync engine must not take at face value
 * (the engine decides from its own baselines): a first sync and a deletion on the server.
 */
@RommServer
@TestInstance(TestInstance.Lifecycle.PER_CLASS)
class NegotiationIntegrationTest {
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

    @Test
    fun withoutSyncHistoryNegotiationFallsBackToTimestampsEvenWhenTheContentDiffers() =
        runBlocking<Unit> {
            val romId = env.romId(fixtureRom(FixtureSystem.SNES))
            val local = saveBytes("local first sync")
            deviceA.client.uploadSave(saveUpload(deviceA, romId, saveBytes("server first sync")))

            val newerLocal = clientState(romId, local, wholeSecondNow() + 1.hours)
            val olderLocal = clientState(romId, local, wholeSecondNow() - 1.hours)
            val uploadPlan = deviceB.client.negotiate(NegotiateRequest(saves = listOf(newerLocal)))
            val downloadPlan = deviceB.client.negotiate(NegotiateRequest(saves = listOf(olderLocal)))

            assertEquals(SyncAction.UPLOAD, uploadPlan.operations.single { it.romId == romId }.action)
            assertEquals(SyncAction.DOWNLOAD, downloadPlan.operations.single { it.romId == romId }.action)
        }

    @Test
    fun aSaveDeletedOnTheServerIsNegotiatedAsADeleteForADeviceStillHoldingIt() =
        runBlocking<Unit> {
            val romId = env.romId(fixtureRom(FixtureSystem.N64))
            val bytes = saveBytes("to be deleted")
            val stored = deviceA.client.uploadSave(saveUpload(deviceA, romId, bytes))
            deviceB.client.downloadSave(stored.id, deviceB.deviceId)
            deviceB.client.confirmDownload(stored.id, deviceB.deviceId, md5(bytes))
            val deletion = buildJsonObject { put("saves", JsonArray(listOf(JsonPrimitive(stored.id)))) }
            env.admin.postJson("/api/saves/delete", deletion).requireSuccess("Deleting the save")

            val held = clientState(romId, bytes, wholeSecondNow() - 1.hours)
            val plan = deviceB.client.negotiate(NegotiateRequest(saves = listOf(held)))

            val operation = plan.operations.single { it.romId == romId }
            assertEquals(SyncAction.DELETE, operation.action)
            assertNull(operation.saveId)
            deviceB.client.completeSession(plan.sessionId)
        }
}
