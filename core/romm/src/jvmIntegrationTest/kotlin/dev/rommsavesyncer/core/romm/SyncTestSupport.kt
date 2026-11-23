package dev.rommsavesyncer.core.romm

import dev.rommsavesyncer.testing.fixtures.FixtureRom
import dev.rommsavesyncer.testing.fixtures.FixtureRoms
import dev.rommsavesyncer.testing.fixtures.FixtureSystem
import dev.rommsavesyncer.testing.harness.RommEnvironment
import kotlinx.coroutines.delay
import java.security.MessageDigest
import kotlin.time.Clock
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Instant

internal const val SLOT = "autosave"
internal const val EMULATOR = "retroarch"
internal const val FILE_NAME = "Fixture.srm"

private const val SAVE_SIZE = 2048
private const val HASH_MULTIPLIER = 31
private val SERVER_CLOCK_TICK = 1_200.milliseconds

/**
 * Device A and device B, paired once per JVM with the shared harness server (RomM limits how many pairings
 * one address may start per minute, and each takes at least one poll interval).
 */
class TwoDevices private constructor(
    val a: SimulatedDevice,
    val b: SimulatedDevice,
) {
    companion object {
        private val lock = Any()
        private var shared: TwoDevices? = null

        fun of(env: RommEnvironment): TwoDevices =
            synchronized(lock) {
                shared ?: TwoDevices(SimulatedDevice.pair(env, "Device A"), SimulatedDevice.pair(env, "Device B"))
                    .also { shared = it }
            }
    }
}

internal fun fixtureRom(system: FixtureSystem): FixtureRom = FixtureRoms.all.single { it.system == system }

internal fun md5(bytes: ByteArray): String =
    MessageDigest.getInstance("MD5").digest(bytes).joinToString("") { "%02x".format(it) }

/** Deterministic save contents that differ for each [label]. */
internal fun saveBytes(label: String): ByteArray =
    ByteArray(SAVE_SIZE) { (label.hashCode() + it * HASH_MULTIPLIER).toByte() }

/** The current time at whole-second precision, which is all RomM keeps. */
internal fun wholeSecondNow(): Instant = Instant.fromEpochSeconds(Clock.System.now().epochSeconds)

/** RomM stores timestamps in whole seconds, so wait before a step whose outcome depends on a later one. */
internal suspend fun awaitNextServerSecond() = delay(SERVER_CLOCK_TICK)

internal fun clientState(
    romId: Int,
    bytes: ByteArray,
    updatedAt: Instant,
    slot: String? = SLOT,
) = ClientSaveState(romId, FILE_NAME, slot, EMULATOR, md5(bytes), updatedAt, bytes.size.toLong())

internal fun saveUpload(
    device: SimulatedDevice,
    romId: Int,
    bytes: ByteArray,
    slot: String? = SLOT,
    sessionId: Int? = null,
    overwrite: Boolean = false,
) = SaveUpload(
    romId = romId,
    file = SaveFile(FILE_NAME, bytes),
    slot = slot,
    emulator = EMULATOR,
    deviceId = device.deviceId,
    sessionId = sessionId,
    contentHash = md5(bytes),
    overwrite = overwrite,
)
