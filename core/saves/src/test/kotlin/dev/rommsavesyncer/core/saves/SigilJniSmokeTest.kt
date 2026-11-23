package dev.rommsavesyncer.core.saves

import com.nendo.sigil.Sigil
import kotlin.test.Test
import kotlin.test.assertEquals

/** Loads the host-built sigil JNI library and calls through it; fails if the native build is missing or broken. */
class SigilJniSmokeTest {
    @Test
    fun jniReportsThePinnedSigilVersion() {
        assertEquals(PINNED_SIGIL_VERSION, Sigil.version())
    }

    private companion object {
        /** The version of the `third_party/argosy-sigil` submodule, pinned at tag v1.0.0. */
        const val PINNED_SIGIL_VERSION = "1.0.0"
    }
}
