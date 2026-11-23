package dev.rommsavesyncer.desktop.app

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.runComposeUiTest
import dev.rommsavesyncer.ui.shared.HelloScreen
import kotlin.test.Test

@OptIn(ExperimentalTestApi::class)
class HelloScreenTest {
    @Test
    fun helloScreenShowsTheAppName() =
        runComposeUiTest {
            setContent { HelloScreen() }

            onNodeWithText("RommSaveSyncer").assertIsDisplayed()
        }
}
