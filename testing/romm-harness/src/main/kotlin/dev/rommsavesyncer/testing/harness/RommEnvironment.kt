package dev.rommsavesyncer.testing.harness

import dev.rommsavesyncer.testing.fixtures.FixtureRom
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonPrimitive

/**
 * A bootstrapped RomM server: the admin user exists and the fixture library has been scanned.
 *
 * [roms] holds RomM's own record for each fixture ROM, as `GET /api/roms` returned it after the scan.
 */
class RommEnvironment(
    val baseUrl: String,
    val admin: RommAdminClient,
    val roms: Map<FixtureRom, JsonObject>,
) {
    fun romId(rom: FixtureRom): Int = record(rom).getValue("id").jsonPrimitive.int

    private fun record(rom: FixtureRom): JsonObject = roms[rom] ?: error("$rom is not part of the fixture library")
}
