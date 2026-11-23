import dev.detekt.gradle.extensions.DetektExtension
import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.kotlin.dsl.configure
import org.jlleitschuh.gradle.ktlint.KtlintExtension

/** ktlint and detekt on every module; both run as part of `check`. */
class LintConventionPlugin : Plugin<Project> {
    override fun apply(target: Project) {
        with(target) {
            pluginManager.apply("org.jlleitschuh.gradle.ktlint")
            pluginManager.apply("dev.detekt")

            extensions.configure<KtlintExtension> {
                version.set(libs.findVersion("ktlint").get().requiredVersion)
                filter {
                    // Vendored sources (the sigil Kotlin binding) are not ours to format.
                    exclude { it.file.path.replace('\\', '/').contains("/third_party/") }
                }
            }

            extensions.configure<DetektExtension> {
                buildUponDefaultConfig.set(true)
                config.setFrom(rootProject.file("config/detekt/detekt.yml"))
                source.setFrom(layout.projectDirectory.dir("src"))
            }
        }
    }
}
