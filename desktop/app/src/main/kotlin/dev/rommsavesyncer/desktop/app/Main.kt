package dev.rommsavesyncer.desktop.app

import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application
import dev.rommsavesyncer.core.model.AppInfo
import dev.rommsavesyncer.ui.shared.HelloScreen

fun main() =
    application {
        Window(onCloseRequest = ::exitApplication, title = AppInfo.NAME) {
            HelloScreen()
        }
    }
