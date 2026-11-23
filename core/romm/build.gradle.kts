plugins {
    id("rommsavesyncer.kmp-library")
    id("rommsavesyncer.integration-test")
    id("rommsavesyncer.serialization")
}

kotlin {
    sourceSets {
        commonMain.dependencies {
            api(libs.ktor.client.core)
            api(libs.kotlinx.serialization.json)
            implementation(libs.ktor.client.content.negotiation)
            implementation(libs.ktor.serialization.kotlinx.json)
        }
        // OkHttp is the engine on both platforms; the shared client takes any engine, so tests use MockEngine.
        jvmMain.dependencies {
            implementation(libs.ktor.client.okhttp)
        }
        getByName("androidMain").dependencies {
            implementation(libs.ktor.client.okhttp)
        }
        jvmTest.dependencies {
            implementation(libs.kotlin.test)
            implementation(libs.ktor.client.mock)
        }
    }
}

dependencies {
    // The Docker-backed tests in src/jvmIntegrationTest: `./gradlew :core:romm:integrationTest`.
    "jvmIntegrationTestImplementation"(project(":testing:romm-harness"))
}
