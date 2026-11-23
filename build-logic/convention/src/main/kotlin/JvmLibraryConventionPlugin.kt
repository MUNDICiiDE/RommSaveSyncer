import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.kotlin.dsl.configure
import org.jetbrains.kotlin.gradle.dsl.KotlinJvmProjectExtension

/** A JVM-only Kotlin module (desktop code, IPC, the sigil adapter). */
class JvmLibraryConventionPlugin : Plugin<Project> {
    override fun apply(target: Project) {
        with(target) {
            pluginManager.apply("org.jetbrains.kotlin.jvm")
            pluginManager.apply("rommsavesyncer.lint")

            extensions.configure<KotlinJvmProjectExtension> {
                jvmToolchain(JAVA_TOOLCHAIN_VERSION)
            }
        }
    }
}
