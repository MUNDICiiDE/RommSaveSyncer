package dev.rommsavesyncer.core.romm

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

class RommVersionTest {
    @Test
    fun parsesTheNumericCoreAndIgnoresSuffixes() {
        assertEquals(RommVersion(5, 4, 1), RommVersion.parse("5.4.1"))
        assertEquals(RommVersion(5, 5, 0), RommVersion.parse("v5.5.0-beta.2"))
        assertEquals(RommVersion(5, 4, 1), RommVersion.parse(" 5.4.1+build7 "))
    }

    @Test
    fun rejectsTextWithoutAThreePartVersion() {
        assertNull(RommVersion.parse("<unknown>"))
        assertNull(RommVersion.parse("5.4"))
        assertNull(RommVersion.parse(""))
    }

    @Test
    fun comparesNumericallyNotAsText() {
        assertTrue(RommVersion(5, 4, 0) < RommVersion.MINIMUM)
        assertTrue(RommVersion(5, 10, 0) > RommVersion(5, 9, 9))
        assertTrue(RommVersion(6, 0, 0) > RommVersion(5, 99, 99))
    }

    @Test
    fun theMinimumIsTheDocumentedConstant() {
        assertEquals("5.4.1", MINIMUM_ROMM_VERSION)
        assertEquals(RommVersion(5, 4, 1), RommVersion.MINIMUM)
    }
}
