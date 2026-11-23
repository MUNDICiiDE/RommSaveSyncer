import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.api.tasks.testing.Test
import org.gradle.jvm.toolchain.JavaLanguageVersion
import org.gradle.jvm.toolchain.JavaToolchainService
import org.gradle.process.CommandLineArgumentProvider
import org.gradle.kotlin.dsl.getByType
import org.gradle.kotlin.dsl.register
import org.gradle.kotlin.dsl.withType

/** Builds sigil's host JNI library and points every test task's `java.library.path` at it. */
class SigilHostJniConventionPlugin : Plugin<Project> {
    override fun apply(target: Project) {
        with(target) {
            val sigilDir = rootProject.layout.projectDirectory.dir("third_party/argosy-sigil")
            val toolchains = extensions.getByType<JavaToolchainService>()

            val buildTask = tasks.register<BuildSigilHostJni>("buildSigilHostJni") {
                description = "Builds sigil's JNI library for the host JDK with CMake."
                group = "build"
                this.sigilDir.set(sigilDir)
                msvcCompatDir.set(rootProject.layout.projectDirectory.dir("native/msvc-compat"))
                sigilSources.from(
                    rootProject.fileTree(sigilDir) {
                        include("CMakeLists.txt", "include/**", "src/**", "third_party/**")
                        include("bindings/android/src/main/cpp/**")
                    },
                )
                cmakeOverride.set(
                    providers.gradleProperty("cmake.path").orElse(providers.environmentVariable("CMAKE")),
                )
                javaHome.set(
                    toolchains.launcherFor { languageVersion.set(JavaLanguageVersion.of(JAVA_TOOLCHAIN_VERSION)) }
                        .map { it.metadata.installationPath.asFile.absolutePath },
                )
                cmakeBuildDir.set(layout.buildDirectory.dir("sigil-cmake"))
                libraryDir.set(layout.buildDirectory.dir("sigil-jni"))
            }

            tasks.withType<Test>().configureEach {
                val libraryDir = buildTask.flatMap { it.libraryDir }
                inputs.dir(libraryDir).withPropertyName("sigilHostJni")
                jvmArgumentProviders.add(
                    CommandLineArgumentProvider { listOf("-Djava.library.path=${libraryDir.get().asFile.absolutePath}") },
                )
            }
        }
    }
}
