import org.gradle.api.Plugin
import org.gradle.api.Project

/** The kotlinx.serialization compiler plugin, for modules whose classes are `@Serializable`. */
class SerializationConventionPlugin : Plugin<Project> {
    override fun apply(target: Project) {
        target.pluginManager.apply("org.jetbrains.kotlin.plugin.serialization")
    }
}
