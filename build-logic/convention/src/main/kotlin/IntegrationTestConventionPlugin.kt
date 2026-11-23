import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.api.file.FileCollection
import org.gradle.api.tasks.SourceSetContainer
import org.gradle.api.tasks.testing.Test
import org.gradle.api.tasks.testing.logging.TestExceptionFormat
import org.gradle.kotlin.dsl.getByType
import org.gradle.kotlin.dsl.register
import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import org.jetbrains.kotlin.gradle.dsl.KotlinJvmProjectExtension
import org.jetbrains.kotlin.gradle.dsl.KotlinMultiplatformExtension

/**
 * An `integrationTest` source set and task (JUnit Jupiter) next to the unit tests.
 * They need Docker, so they are not part of `check`; CI runs `./gradlew integrationTest -Promm=docker`.
 *
 * On a JVM module the source set is `src/integrationTest` and its configurations are `integrationTest*`.
 * On a Kotlin Multiplatform module (apply `rommsavesyncer.kmp-library` first) it is a `jvm` target compilation,
 * so the sources are in `src/jvmIntegrationTest` and the configurations are `jvmIntegrationTest*`.
 */
class IntegrationTestConventionPlugin : Plugin<Project> {
    override fun apply(target: Project) {
        with(target) {
            val multiplatform = pluginManager.hasPlugin("org.jetbrains.kotlin.multiplatform")
            val prefix = if (multiplatform) "jvmIntegrationTest" else INTEGRATION_TEST
            val mainImplementation = if (multiplatform) "jvmMainImplementation" else "implementation"
            val mainRuntimeOnly = if (multiplatform) "jvmMainRuntimeOnly" else "runtimeOnly"
            val unitTestTask = if (multiplatform) "jvmTest" else "test"
            val inputs = if (multiplatform) configureMultiplatformCompilation() else configureJvmSourceSet()

            configurations.getByName("${prefix}Implementation")
                .extendsFrom(configurations.getByName(mainImplementation))
            configurations.getByName("${prefix}RuntimeOnly")
                .extendsFrom(configurations.getByName(mainRuntimeOnly))

            dependencies.add("${prefix}Implementation", dependencies.platform(libs.findLibrary("junit-bom").get()))
            dependencies.add("${prefix}Implementation", libs.findLibrary("junit-jupiter").get())
            dependencies.add("${prefix}RuntimeOnly", libs.findLibrary("junit-platform-launcher").get())

            tasks.register<Test>(INTEGRATION_TEST) {
                group = "verification"
                description = "Runs the Docker-backed RomM integration tests (needs a running Docker daemon)."
                testClassesDirs = inputs.classesDirs
                classpath = inputs.classpath
                useJUnitPlatform()
                shouldRunAfter(tasks.named(unitTestTask))
                // Docker state is outside Gradle's view, so never treat a previous run as up to date.
                outputs.upToDateWhen { false }
                testLogging {
                    events("passed", "failed", "skipped")
                    exceptionFormat = TestExceptionFormat.FULL
                }
            }
        }
    }

    /** What the `integrationTest` task runs: the compiled test classes and everything on their classpath. */
    private class TestInputs(
        val classesDirs: FileCollection,
        val classpath: FileCollection,
    )

    private fun Project.configureJvmSourceSet(): TestInputs {
        pluginManager.apply("rommsavesyncer.jvm-library")

        val integrationTest = extensions.getByType<SourceSetContainer>().create(INTEGRATION_TEST)
        val kotlin = extensions.getByType<KotlinJvmProjectExtension>()
        kotlin.target.compilations.getByName(INTEGRATION_TEST)
            .associateWith(kotlin.target.compilations.getByName("main"))
        return TestInputs(integrationTest.output.classesDirs, integrationTest.runtimeClasspath)
    }

    private fun Project.configureMultiplatformCompilation(): TestInputs {
        val kotlin = extensions.getByType<KotlinMultiplatformExtension>()
        lateinit var inputs: TestInputs
        kotlin.jvm {
            val integrationTest = compilations.create(INTEGRATION_TEST)
            integrationTest.associateWith(compilations.getByName("main"))
            // The harness is built for the toolchain's JVM, which the shared library target (17) must not undercut.
            integrationTest.compileTaskProvider.configure { compilerOptions.jvmTarget.set(JvmTarget.JVM_21) }
            inputs =
                TestInputs(
                    integrationTest.output.classesDirs,
                    this@configureMultiplatformCompilation.files(
                        integrationTest.compileDependencyFiles,
                        integrationTest.runtimeDependencyFiles,
                        integrationTest.output.allOutputs,
                    ),
                )
        }
        return inputs
    }

    private companion object {
        const val INTEGRATION_TEST = "integrationTest"
    }
}
