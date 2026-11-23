import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.api.tasks.SourceSetContainer
import org.gradle.api.tasks.testing.Test
import org.gradle.api.tasks.testing.logging.TestExceptionFormat
import org.gradle.kotlin.dsl.getByType
import org.gradle.kotlin.dsl.register
import org.jetbrains.kotlin.gradle.dsl.KotlinJvmProjectExtension

/**
 * An `integrationTest` source set and task (JUnit Jupiter) next to the unit tests.
 * They need Docker, so they are not part of `check`; CI runs `./gradlew integrationTest -Promm=docker`.
 */
class IntegrationTestConventionPlugin : Plugin<Project> {
    override fun apply(target: Project) {
        with(target) {
            pluginManager.apply("rommsavesyncer.jvm-library")

            val sourceSets = extensions.getByType<SourceSetContainer>()
            val integrationTest = sourceSets.create(INTEGRATION_TEST)
            val kotlin = extensions.getByType<KotlinJvmProjectExtension>()
            kotlin.target.compilations.getByName(INTEGRATION_TEST)
                .associateWith(kotlin.target.compilations.getByName("main"))

            configurations.getByName("${INTEGRATION_TEST}Implementation")
                .extendsFrom(configurations.getByName("implementation"))
            configurations.getByName("${INTEGRATION_TEST}RuntimeOnly")
                .extendsFrom(configurations.getByName("runtimeOnly"))

            dependencies.add(
                "${INTEGRATION_TEST}Implementation",
                dependencies.platform(libs.findLibrary("junit-bom").get()),
            )
            dependencies.add("${INTEGRATION_TEST}Implementation", libs.findLibrary("junit-jupiter").get())
            dependencies.add("${INTEGRATION_TEST}RuntimeOnly", libs.findLibrary("junit-platform-launcher").get())

            tasks.register<Test>(INTEGRATION_TEST) {
                group = "verification"
                description = "Runs the Docker-backed RomM integration tests (needs a running Docker daemon)."
                testClassesDirs = integrationTest.output.classesDirs
                classpath = integrationTest.runtimeClasspath
                useJUnitPlatform()
                shouldRunAfter(tasks.named("test"))
                // Docker state is outside Gradle's view, so never treat a previous run as up to date.
                outputs.upToDateWhen { false }
                testLogging {
                    events("passed", "failed", "skipped")
                    exceptionFormat = TestExceptionFormat.FULL
                }
            }
        }
    }

    private companion object {
        const val INTEGRATION_TEST = "integrationTest"
    }
}
