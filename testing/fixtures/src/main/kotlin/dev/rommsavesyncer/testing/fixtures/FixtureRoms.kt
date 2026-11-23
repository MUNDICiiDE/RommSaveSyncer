package dev.rommsavesyncer.testing.fixtures

import java.nio.file.Files
import java.nio.file.Path

/** The fixture ROM library: one synthetic ROM per [FixtureSystem]. */
object FixtureRoms {
    val all: List<FixtureRom> = FixtureSystem.entries.map(::FixtureRom)

    /** Writes the library as RomM expects it under its library root: `<root>/roms/<platform>/<file>`. */
    fun writeLibrary(root: Path) {
        for (rom in all) {
            val file = root.resolve(rom.libraryPath)
            Files.createDirectories(file.parent)
            Files.write(file, rom.content)
        }
    }
}
