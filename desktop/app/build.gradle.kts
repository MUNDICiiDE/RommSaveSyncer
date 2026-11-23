import org.jetbrains.compose.desktop.application.dsl.TargetFormat

plugins {
    id("rommsavesyncer.jvm-library")
    id("rommsavesyncer.compose")
}

dependencies {
    implementation(project(":ui:shared"))
    implementation(compose.desktop.currentOs)

    testImplementation(libs.kotlin.test)
    testImplementation(libs.compose.ui.test)
}

compose.desktop {
    application {
        mainClass = "dev.rommsavesyncer.desktop.app.MainKt"

        buildTypes.release.proguard {
            isEnabled = false
        }

        nativeDistributions {
            targetFormats(TargetFormat.Msi)
            packageName = "RommSaveSyncer"
            packageVersion = "1.0.0"
            description = "Keeps emulator saves in sync with RomM"

            windows {
                menuGroup = "RommSaveSyncer"
                upgradeUuid = "8f1c6c52-3d9e-4b0a-9a57-2f6f4f0b7a31"
            }
        }
    }
}
