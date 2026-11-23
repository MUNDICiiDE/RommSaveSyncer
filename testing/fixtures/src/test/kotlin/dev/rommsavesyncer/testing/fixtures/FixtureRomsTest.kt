package dev.rommsavesyncer.testing.fixtures

import java.nio.file.Files
import kotlin.io.path.readBytes
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class FixtureRomsTest {
    @Test
    fun coversEveryV1SystemWithRommsPlatformSlugs() {
        val slugs = FixtureRoms.all.map { it.system.platformSlug }
        assertEquals(listOf("gb", "gbc", "gba", "nes", "snes", "n64", "nds", "psx", "ngc"), slugs)
    }

    @Test
    fun hashesAreStableAcrossRuns() {
        val gb = FixtureRoms.all.first { it.system == FixtureSystem.GB }
        assertEquals("1ee29115d7c9815b12c3f7b9add409c2", gb.md5)
        assertEquals("b436929fd35f114bc2d519834fef332662d1acf1", gb.sha1)
        assertEquals("113606d7", gb.crc32)
    }

    @Test
    fun everyRomHasDistinctContentAndHashes() {
        assertEquals(
            FixtureRoms.all.size,
            FixtureRoms.all
                .map { it.md5 }
                .toSet()
                .size,
        )
        assertEquals(
            FixtureRoms.all.size,
            FixtureRoms.all
                .map { it.sha1 }
                .toSet()
                .size,
        )
        assertEquals(
            FixtureRoms.all.size,
            FixtureRoms.all
                .map { it.fileName }
                .toSet()
                .size,
        )
    }

    @Test
    fun writeLibraryLaysFilesOutAsRommExpects() {
        val root = Files.createTempDirectory("fixture-library")
        try {
            FixtureRoms.writeLibrary(root)
            for (rom in FixtureRoms.all) {
                assertTrue(rom.libraryPath.startsWith("roms/${rom.system.platformSlug}/"))
                assertContentEquals(rom.content, root.resolve(rom.libraryPath).readBytes())
            }
        } finally {
            root.toFile().deleteRecursively()
        }
    }
}
