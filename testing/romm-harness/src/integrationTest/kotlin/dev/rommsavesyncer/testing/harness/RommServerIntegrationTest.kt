package dev.rommsavesyncer.testing.harness

import dev.rommsavesyncer.testing.fixtures.FixtureRoms
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

@RommServer
class RommServerIntegrationTest {
    @Test
    fun heartbeatReportsThePinnedRommVersion(env: RommEnvironment) {
        val system =
            RommBootstrap
                .heartbeat(env.baseUrl)
                .requireSuccess("Heartbeat")
                .jsonObject()
                .getValue("SYSTEM")

        assertEquals(
            "5.4.1",
            system.jsonObject
                .getValue("VERSION")
                .jsonPrimitive.content,
        )
        assertEquals("5.4.1", ROMM_IMAGE.substringAfter(':'))
    }

    @Test
    fun theSetupWizardIsDoneOnceTheAdminExists(env: RommEnvironment) {
        val system =
            RommBootstrap
                .heartbeat(env.baseUrl)
                .jsonObject()
                .getValue("SYSTEM")
                .jsonObject

        assertFalse(
            system
                .getValue("SHOW_SETUP_WIZARD")
                .jsonPrimitive.content
                .toBoolean(),
        )
    }

    @Test
    fun everyFixtureRomGetsADistinctRomIdAndItsExpectedHashes(env: RommEnvironment) {
        assertEquals(
            FixtureRoms.all.size,
            FixtureRoms.all
                .map { env.romId(it) }
                .toSet()
                .size,
        )

        for (rom in FixtureRoms.all) {
            val record = env.roms.getValue(rom)
            assertEquals(rom.system.platformSlug, record.getValue("platform_slug").jsonPrimitive.content, "$rom")
            assertEquals(rom.md5, record.getValue("md5_hash").jsonPrimitive.content, "$rom")
            assertEquals(rom.sha1, record.getValue("sha1_hash").jsonPrimitive.content, "$rom")
            assertEquals(rom.crc32, record.getValue("crc_hash").jsonPrimitive.content, "$rom")
        }
    }

    @Test
    fun everyFixtureRomIsFoundByEachOfItsHashes(env: RommEnvironment) {
        for (rom in FixtureRoms.all) {
            for (query in listOf("md5_hash=${rom.md5}", "sha1_hash=${rom.sha1}", "crc_hash=${rom.crc32}")) {
                val found =
                    env.admin
                        .get("/api/roms/by-hash?$query")
                        .requireSuccess("by-hash $query")
                        .jsonObject()

                assertEquals(
                    env.romId(rom),
                    found
                        .getValue("id")
                        .jsonPrimitive.content
                        .toInt(),
                    "$rom by $query",
                )
            }
        }
    }

    @Test
    fun theAdminClientCanCallAdminOnlyEndpoints(env: RommEnvironment) {
        assertTrue(env.admin.get("/api/tasks").isSuccess)

        val users =
            env.admin
                .get("/api/users")
                .requireSuccess("Listing users")
                .json()
                .jsonArray
        assertEquals(
            listOf("admin"),
            users.map {
                it.jsonObject
                    .getValue("username")
                    .jsonPrimitive.content
            },
        )
    }

    @Test
    fun aFreshServerHasNoSavesOrDevices(env: RommEnvironment) {
        assertTrue(
            env.admin
                .get("/api/saves")
                .requireSuccess("Listing saves")
                .json()
                .jsonArray
                .isEmpty(),
        )
        assertTrue(
            env.admin
                .get("/api/devices")
                .requireSuccess("Listing devices")
                .json()
                .jsonArray
                .isEmpty(),
        )
    }
}
