import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.file.ConfigurableFileCollection
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.InputDirectory
import org.gradle.api.tasks.InputFiles
import org.gradle.api.tasks.Internal
import org.gradle.api.tasks.LocalState
import org.gradle.api.tasks.Optional
import org.gradle.api.tasks.OutputDirectory
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction
import org.gradle.work.DisableCachingByDefault
import org.gradle.process.ExecOperations
import java.io.File
import javax.inject.Inject

/**
 * Builds sigil's JNI library for the host JDK with CMake (`-DSIGIL_BUILD_JNI_HOST=ON`, MSVC on Windows,
 * gcc on Linux) and copies it to [libraryDir] so tests can point `java.library.path` at one stable folder.
 */
@DisableCachingByDefault(because = "The output is a machine-specific native binary")
abstract class BuildSigilHostJni @Inject constructor(
    private val execOperations: ExecOperations,
) : DefaultTask() {
    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val sigilSources: ConfigurableFileCollection

    @get:Input
    @get:Optional
    abstract val cmakeOverride: Property<String>

    @get:Input
    abstract val javaHome: Property<String>

    @get:Internal
    abstract val sigilDir: DirectoryProperty

    /** Holds a stand-in `unistd.h`; put on the include path for MSVC only. */
    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val msvcCompatDir: DirectoryProperty

    @get:LocalState
    abstract val cmakeBuildDir: DirectoryProperty

    @get:OutputDirectory
    abstract val libraryDir: DirectoryProperty

    @TaskAction
    fun build() {
        val source = sigilDir.get().asFile
        if (!File(source, "CMakeLists.txt").isFile) {
            throw GradleException("sigil submodule is empty: run `git submodule update --init` ($source)")
        }
        val cmake = CmakeLocator.locate(cmakeOverride.orNull)
        // Always configure from scratch: the compiler flags below only take effect on a fresh CMake cache.
        val buildDir = cmakeBuildDir.get().asFile.also { it.deleteRecursively() }

        val configureEnvironment = if (isWindows()) mapOf("CFLAGS" to "/I\"${msvcCompatDir.get().asFile}\"") else emptyMap()
        cmake(cmake, "-S", source.absolutePath, "-B", buildDir.absolutePath, *CONFIGURE_ARGS, environment = configureEnvironment)
        cmake(cmake, "--build", buildDir.absolutePath, "--config", "Release", "--target", TARGET, "--parallel")

        val library = findLibrary(File(buildDir, "bindings"))
        val outputDir = libraryDir.get().asFile
        outputDir.deleteRecursively()
        outputDir.mkdirs()
        library.copyTo(File(outputDir, library.name))
    }

    private fun cmake(vararg command: String, environment: Map<String, String> = emptyMap()) {
        execOperations.exec {
            commandLine(*command)
            environment("JAVA_HOME", javaHome.get())
            environment(environment)
        }
    }

    private fun isWindows() = System.getProperty("os.name").startsWith("Windows")

    private fun findLibrary(bindingsDir: File): File =
        bindingsDir.walkTopDown().firstOrNull { it.isFile && it.name in LIBRARY_NAMES }
            ?: throw GradleException("sigil JNI library not found under $bindingsDir (looked for $LIBRARY_NAMES)")

    private companion object {
        const val TARGET = "sigil_jni_host"
        val LIBRARY_NAMES = setOf("sigil-jni.dll", "libsigil-jni.so", "libsigil-jni.dylib")
        val CONFIGURE_ARGS = arrayOf(
            "-DSIGIL_BUILD_JNI_HOST=ON",
            "-DSIGIL_BUILD_CLI=OFF",
            "-DSIGIL_BUILD_TESTS=OFF",
            "-DCMAKE_BUILD_TYPE=Release",
        )
    }
}
