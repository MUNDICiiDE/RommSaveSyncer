import java.io.File

/**
 * Finds a CMake executable. Order: explicit override (`-Pcmake.path=` or the `CMAKE` environment variable),
 * then on Windows the CMake bundled with the newest Visual Studio (it knows that VS's generator),
 * then `cmake` on the PATH.
 */
internal object CmakeLocator {
    private const val BUNDLED_CMAKE_PATH = "Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe"
    private const val VC_TOOLS_COMPONENT = "Microsoft.VisualStudio.Component.VC.Tools.x86.x64"

    fun locate(override: String?): String {
        if (!override.isNullOrBlank()) {
            require(File(override).isFile) { "cmake override '$override' is not a file" }
            return override
        }
        if (isWindows()) {
            visualStudioCmake()?.let { return it.absolutePath }
        }
        return "cmake"
    }

    private fun isWindows() = System.getProperty("os.name").startsWith("Windows")

    private fun visualStudioCmake(): File? {
        val installer = System.getenv("ProgramFiles(x86)") ?: return null
        val vswhere = File(installer, "Microsoft Visual Studio/Installer/vswhere.exe")
        if (!vswhere.isFile) return null
        val process = ProcessBuilder(
            vswhere.absolutePath,
            "-latest",
            "-products",
            "*",
            "-requires",
            VC_TOOLS_COMPONENT,
            "-property",
            "installationPath",
        ).redirectErrorStream(true).start()
        val installPath = process.inputStream.bufferedReader().readText().trim()
        process.waitFor()
        return File(installPath, BUNDLED_CMAKE_PATH).takeIf { installPath.isNotEmpty() && it.isFile }
    }
}
