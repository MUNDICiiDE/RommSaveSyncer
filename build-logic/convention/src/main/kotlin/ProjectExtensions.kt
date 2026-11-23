import org.gradle.api.Project
import org.gradle.api.artifacts.VersionCatalog
import org.gradle.api.artifacts.VersionCatalogsExtension
import org.gradle.kotlin.dsl.getByType

internal const val JAVA_TOOLCHAIN_VERSION = 21
internal const val SHARED_JVM_TARGET = 17
internal const val APP_PACKAGE = "dev.rommsavesyncer"

// androidx.compose 1.12 needs compileSdk 37 (AGP downloads it); minSdk 33 is Android 13 (the AYN Thor).
internal const val ANDROID_COMPILE_SDK = 37
internal const val ANDROID_MIN_SDK = 33
internal const val ANDROID_BUILD_TOOLS = "36.0.0"

internal val Project.libs: VersionCatalog
    get() = extensions.getByType<VersionCatalogsExtension>().named("libs")

/** `core:model` becomes `dev.rommsavesyncer.core.model`. */
internal val Project.moduleNamespace: String
    get() = "$APP_PACKAGE${path.replace(':', '.').replace('-', '_')}"
