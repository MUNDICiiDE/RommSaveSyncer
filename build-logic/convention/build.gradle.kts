plugins {
    `kotlin-dsl`
}

group = "dev.rommsavesyncer.buildlogic"

java {
    toolchain.languageVersion.set(JavaLanguageVersion.of(21))
}

dependencies {
    implementation(libs.kotlin.gradle.plugin)
    implementation(libs.kotlin.compose.compiler.plugin)
    implementation(libs.compose.gradle.plugin)
    implementation(libs.android.gradle.plugin)
    implementation(libs.ktlint.gradle.plugin)
    implementation(libs.detekt.gradle.plugin)
}

gradlePlugin {
    plugins {
        register("lint") {
            id = "rommsavesyncer.lint"
            implementationClass = "LintConventionPlugin"
        }
        register("jvmLibrary") {
            id = "rommsavesyncer.jvm-library"
            implementationClass = "JvmLibraryConventionPlugin"
        }
        register("kmpLibrary") {
            id = "rommsavesyncer.kmp-library"
            implementationClass = "KmpLibraryConventionPlugin"
        }
        register("compose") {
            id = "rommsavesyncer.compose"
            implementationClass = "ComposeConventionPlugin"
        }
        register("androidApp") {
            id = "rommsavesyncer.android-app"
            implementationClass = "AndroidAppConventionPlugin"
        }
        register("integrationTest") {
            id = "rommsavesyncer.integration-test"
            implementationClass = "IntegrationTestConventionPlugin"
        }
        register("sigilHostJni") {
            id = "rommsavesyncer.sigil-host-jni"
            implementationClass = "SigilHostJniConventionPlugin"
        }
    }
}
