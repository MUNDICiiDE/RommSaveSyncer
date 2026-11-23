plugins {
    id("rommsavesyncer.kmp-library")
}

kotlin {
    sourceSets {
        jvmTest.dependencies {
            implementation(libs.kotlin.test)
        }
    }
}
