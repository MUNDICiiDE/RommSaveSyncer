package dev.rommsavesyncer.testing.fixtures

/**
 * The v1 systems with a fixture ROM.
 *
 * [platformSlug] is the folder name RomM maps to a platform (`UniversalPlatformSlug` in RomM 5.4.1's
 * `backend/utils/platform_slugs.py`), so the fixture library needs no `config.yml`.
 */
enum class FixtureSystem(
    val label: String,
    val platformSlug: String,
    val extension: String,
) {
    GB("GB", "gb", "gb"),
    GBC("GBC", "gbc", "gbc"),
    GBA("GBA", "gba", "gba"),
    NES("NES", "nes", "nes"),
    SNES("SNES", "snes", "sfc"),
    N64("N64", "n64", "z64"),
    NDS("DS", "nds", "nds"),
    PS1("PS1", "psx", "bin"),
    GAMECUBE("GameCube", "ngc", "iso"),
}
