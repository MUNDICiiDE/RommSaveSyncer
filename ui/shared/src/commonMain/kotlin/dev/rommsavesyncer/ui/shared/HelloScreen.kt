package dev.rommsavesyncer.ui.shared

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.sp
import dev.rommsavesyncer.core.model.AppInfo

private const val TITLE_FONT_SIZE_SP = 32

/** The M1 hello-world screen, shared by the desktop window and the Android activity. */
@Composable
fun HelloScreen() {
    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        BasicText(text = AppInfo.NAME, style = TextStyle(fontSize = TITLE_FONT_SIZE_SP.sp))
    }
}
