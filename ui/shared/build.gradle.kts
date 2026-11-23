plugins {
    id("rommsavesyncer.kmp-library")
    id("rommsavesyncer.compose")
}

kotlin {
    sourceSets {
        commonMain.dependencies {
            api(project(":core:model"))
            implementation(libs.compose.runtime)
            implementation(libs.compose.foundation)
        }
    }
}
