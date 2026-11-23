package dev.rommsavesyncer.testing.fixtures

import java.security.MessageDigest
import java.util.zip.CRC32

/**
 * A synthetic ROM: not a real game image, just deterministic bytes with the right extension.
 * The hashes use RomM's formats (lowercase hex; CRC32 is 8 digits).
 */
class FixtureRom(
    val system: FixtureSystem,
) {
    val title: String = "Fixture ${system.label}"
    val fileName: String = "$title.${system.extension}"

    /** Path relative to the library root RomM is given (`roms/<platform>/<file>`). */
    val libraryPath: String = "$ROMS_DIR/${system.platformSlug}/$fileName"

    val content: ByteArray = syntheticContent()
    val md5: String = digest("MD5")
    val sha1: String = digest("SHA-1")
    val crc32: String = "%08x".format(CRC32().also { it.update(content) }.value)

    private fun syntheticContent(): ByteArray {
        val header = "ROMMSAVESYNCER-FIXTURE|${system.platformSlug}|$title\n".toByteArray(Charsets.US_ASCII)
        return ByteArray(SIZE_BYTES) { header[it % header.size] }
    }

    private fun digest(algorithm: String): String =
        MessageDigest.getInstance(algorithm).digest(content).joinToString("") { "%02x".format(it) }

    override fun toString(): String = libraryPath

    companion object {
        const val ROMS_DIR = "roms"
        const val SIZE_BYTES = 32 * 1024
    }
}
