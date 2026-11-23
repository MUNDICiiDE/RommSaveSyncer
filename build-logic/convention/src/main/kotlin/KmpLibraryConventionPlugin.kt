import com.android.build.api.dsl.KotlinMultiplatformAndroidLibraryTarget
import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.api.plugins.ExtensionAware
import org.gradle.kotlin.dsl.configure
import org.gradle.kotlin.dsl.getByType
import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import org.jetbrains.kotlin.gradle.dsl.KotlinMultiplatformExtension

/** A Kotlin Multiplatform module shared by the desktop (JVM) and Android apps. */
class KmpLibraryConventionPlugin : Plugin<Project> {
    override fun apply(target: Project) {
        with(target) {
            pluginManager.apply("org.jetbrains.kotlin.multiplatform")
            pluginManager.apply("com.android.kotlin.multiplatform.library")
            pluginManager.apply("rommsavesyncer.lint")

            val namespace = moduleNamespace
            extensions.configure<KotlinMultiplatformExtension> {
                jvmToolchain(JAVA_TOOLCHAIN_VERSION)
                jvm {
                    compilerOptions.jvmTarget.set(JvmTarget.fromTarget(SHARED_JVM_TARGET.toString()))
                }
                (this as ExtensionAware).extensions.getByType<KotlinMultiplatformAndroidLibraryTarget>().apply {
                    this.namespace = namespace
                    compileSdk { version = release(ANDROID_COMPILE_SDK) }
                    minSdk = ANDROID_MIN_SDK
                    compilerOptions { jvmTarget.set(JvmTarget.fromTarget(SHARED_JVM_TARGET.toString())) }
                }
            }
        }
    }
}
