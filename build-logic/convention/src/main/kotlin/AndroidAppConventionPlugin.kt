import com.android.build.api.dsl.ApplicationExtension
import org.gradle.api.JavaVersion
import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.kotlin.dsl.configure

/** The Android application module. AGP 9 compiles Kotlin itself, so no Kotlin Android plugin. */
class AndroidAppConventionPlugin : Plugin<Project> {
    override fun apply(target: Project) {
        with(target) {
            pluginManager.apply("com.android.application")
            pluginManager.apply("rommsavesyncer.lint")

            extensions.configure<ApplicationExtension> {
                namespace = APP_PACKAGE
                compileSdk { version = release(ANDROID_COMPILE_SDK) }
                buildToolsVersion = ANDROID_BUILD_TOOLS

                defaultConfig {
                    applicationId = APP_PACKAGE
                    minSdk = ANDROID_MIN_SDK
                    targetSdk = ANDROID_COMPILE_SDK
                    versionCode = 1
                    versionName = "1.0.0"
                }

                buildTypes {
                    debug {
                        // A separate applicationId and data directory for debug builds.
                        applicationIdSuffix = ".debug"
                    }
                }

                compileOptions {
                    sourceCompatibility = JavaVersion.VERSION_17
                    targetCompatibility = JavaVersion.VERSION_17
                }

                lint {
                    abortOnError = true
                    checkReleaseBuilds = false
                }
            }
        }
    }
}
