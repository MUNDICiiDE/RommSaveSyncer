plugins {
    id("rommsavesyncer.jvm-library")
    id("rommsavesyncer.integration-test")
}

dependencies {
    api(platform(libs.junit.bom))
    api(libs.junit.jupiter.api)
    api(libs.kotlinx.serialization.json)
    api(project(":testing:fixtures"))
    implementation(libs.testcontainers)
}

// The fixture ROMs for the docker-compose.yml stack (manual LAN testing); gitignored.
tasks.register<JavaExec>("writeFixtureLibrary") {
    group = "romm harness"
    description = "Writes the fixture ROM library to manual/library for docker-compose.yml."
    classpath = sourceSets["main"].runtimeClasspath
    mainClass = "dev.rommsavesyncer.testing.harness.WriteFixtureLibraryKt"
    args(
        layout.projectDirectory
            .dir("manual/library")
            .asFile.path,
    )
}
