package dev.rommsavesyncer.testing.harness

import dev.rommsavesyncer.testing.fixtures.FixtureRoms
import java.nio.file.Path

/** Writes the fixture ROM library into the directory given as the only argument (`writeFixtureLibrary` task). */
fun main(args: Array<String>) {
    require(args.size == 1) { "Usage: WriteFixtureLibraryKt <library-dir>" }
    FixtureRoms.writeLibrary(Path.of(args[0]))
}
