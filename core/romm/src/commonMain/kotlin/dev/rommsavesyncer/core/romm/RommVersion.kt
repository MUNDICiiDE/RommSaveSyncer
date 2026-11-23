package dev.rommsavesyncer.core.romm

/** The oldest RomM server this app supports; older servers are refused. */
const val MINIMUM_ROMM_VERSION = "5.4.1"

/** The numeric core of a RomM release such as `5.4.1`; a suffix like `-beta.2` is ignored. */
data class RommVersion(
    val major: Int,
    val minor: Int,
    val patch: Int,
) : Comparable<RommVersion> {
    override fun compareTo(other: RommVersion): Int =
        compareValuesBy(this, other, RommVersion::major, RommVersion::minor, RommVersion::patch)

    override fun toString(): String = "$major.$minor.$patch"

    companion object {
        private val PATTERN = Regex("""^v?(\d{1,4})\.(\d{1,4})\.(\d{1,4})""")

        val MINIMUM: RommVersion = checkNotNull(parse(MINIMUM_ROMM_VERSION))

        /** The version in [text], or null when it does not start with `major.minor.patch` (such as `<unknown>`). */
        fun parse(text: String): RommVersion? {
            val groups = PATTERN.find(text.trim())?.groupValues ?: return null
            val (major, minor, patch) = groups.drop(1).map(String::toInt)
            return RommVersion(major, minor, patch)
        }
    }
}
