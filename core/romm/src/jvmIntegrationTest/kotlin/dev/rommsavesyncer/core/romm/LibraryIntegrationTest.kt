package dev.rommsavesyncer.core.romm

import dev.rommsavesyncer.testing.fixtures.FixtureRoms
import dev.rommsavesyncer.testing.fixtures.FixtureSystem
import dev.rommsavesyncer.testing.harness.RommEnvironment
import dev.rommsavesyncer.testing.harness.RommServer
import kotlinx.coroutines.runBlocking
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeAll
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.TestInstance

@RommServer
@TestInstance(TestInstance.Lifecycle.PER_CLASS)
class LibraryIntegrationTest {
    private lateinit var env: RommEnvironment
    private lateinit var device: SimulatedDevice

    @BeforeAll
    fun pairDevice(environment: RommEnvironment) {
        env = environment
        device = TwoDevices.of(env).a
    }

    @Test
    fun aRomIsFoundByItsFileHashesAndAnUnknownHashIsNull() =
        runBlocking<Unit> {
            val rom = fixtureRom(FixtureSystem.PS1)

            val byMd5 = device.client.findRomByHash(md5 = rom.md5)
            val bySha1 = device.client.findRomByHash(sha1 = rom.sha1)
            val byCrc = device.client.findRomByHash(crc = rom.crc32)

            assertEquals(env.romId(rom), byMd5?.id)
            assertEquals(env.romId(rom), bySha1?.id)
            assertEquals(env.romId(rom), byCrc?.id)
            assertEquals(rom.fileName, byMd5?.fileName)
            assertEquals("psx", byMd5?.platformSlug)
            assertNull(device.client.findRomByHash(md5 = "0".repeat(MD5_LENGTH)))
        }

    @Test
    fun listingRomsFindsEveryFixtureAndCanSearchByName() =
        runBlocking<Unit> {
            val all = device.client.listRoms(limit = LIST_LIMIT)
            val gameCube = fixtureRom(FixtureSystem.GAMECUBE)

            val searched = device.client.listRoms(searchTerm = gameCube.title)

            assertEquals(FixtureRoms.all.map { env.romId(it) }.toSet(), all.items.map { it.id }.toSet())
            assertTrue(env.romId(gameCube) in searched.items.map { it.id })
        }

    private companion object {
        const val MD5_LENGTH = 32
        const val LIST_LIMIT = 100
    }
}
