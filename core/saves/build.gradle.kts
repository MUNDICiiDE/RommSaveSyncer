plugins {
    id("rommsavesyncer.jvm-library")
    id("rommsavesyncer.sigil-host-jni")
}

kotlin {
    sourceSets {
        main {
            // sigil's Kotlin/JNI binding, compiled unmodified from the submodule.
            kotlin.srcDir("$rootDir/third_party/argosy-sigil/bindings/android/src/main/kotlin")
        }
    }
}

dependencies {
    testImplementation(libs.kotlin.test)
}
