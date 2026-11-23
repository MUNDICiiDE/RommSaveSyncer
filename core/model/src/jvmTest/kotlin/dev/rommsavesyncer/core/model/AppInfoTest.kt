package dev.rommsavesyncer.core.model

import kotlin.test.Test
import kotlin.test.assertEquals

class AppInfoTest {
    @Test
    fun appNameIsRommSaveSyncer() {
        assertEquals("RommSaveSyncer", AppInfo.NAME)
    }
}
