import org.gradle.api.Plugin
import org.gradle.api.Project

/** Compose Multiplatform plus the Compose compiler plugin; apply after a Kotlin plugin. */
class ComposeConventionPlugin : Plugin<Project> {
    override fun apply(target: Project) {
        with(target) {
            pluginManager.apply("org.jetbrains.compose")
            pluginManager.apply("org.jetbrains.kotlin.plugin.compose")
        }
    }
}
