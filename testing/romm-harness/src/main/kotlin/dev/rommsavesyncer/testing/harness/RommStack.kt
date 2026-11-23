package dev.rommsavesyncer.testing.harness

import dev.rommsavesyncer.testing.fixtures.FixtureRoms
import org.testcontainers.containers.GenericContainer
import org.testcontainers.containers.Network
import org.testcontainers.containers.wait.strategy.Wait
import org.testcontainers.utility.DockerImageName
import org.testcontainers.utility.MountableFile
import java.nio.file.Files
import java.nio.file.Path
import java.time.Duration

/**
 * A throwaway RomM + MariaDB pair with the fixture ROM library mounted, started through Testcontainers.
 *
 * Nothing persists between runs: the containers and their anonymous volumes are removed on [close], and the
 * fixture library is copied in, not bind-mounted. The stack is not bootstrapped; see [RommBootstrap].
 */
class RommStack private constructor(
    private val network: Network,
    private val database: GenericContainer<*>,
    private val romm: GenericContainer<*>,
    private val libraryDir: Path,
) : AutoCloseable {
    val baseUrl: String
        get() = "http://${romm.host}:${romm.getMappedPort(ROMM_PORT)}"

    override fun close() {
        romm.stop()
        database.stop()
        network.close()
        libraryDir.toFile().deleteRecursively()
    }

    companion object {
        private const val ROMM_PORT = 8080
        private const val HTTP_OK = 200
        private const val DB_ALIAS = "romm-db"
        private const val DB_NAME = "romm"
        private const val DB_USER = "romm-user"
        private const val DB_PASSWORD = "romm-test-password"

        // A throwaway key; this instance only ever holds synthetic data.
        private const val AUTH_SECRET_KEY = "0000000000000000000000000000000000000000000000000000000000000001"
        private val DATABASE_START_TIMEOUT = Duration.ofMinutes(2)
        private val ROMM_START_TIMEOUT = Duration.ofMinutes(5)

        fun start(): RommStack {
            val libraryDir = Files.createTempDirectory("romm-library")
            FixtureRoms.writeLibrary(libraryDir)
            val network = Network.newNetwork()
            val stack = RommStack(network, databaseContainer(network), rommContainer(network, libraryDir), libraryDir)
            var started = false
            try {
                stack.database.start()
                stack.romm.start()
                started = true
            } finally {
                if (!started) stack.close()
            }
            return stack
        }

        private fun databaseContainer(network: Network): GenericContainer<*> =
            GenericContainer(DockerImageName.parse(MARIADB_IMAGE))
                .withNetwork(network)
                .withNetworkAliases(DB_ALIAS)
                .withEnv("MARIADB_ROOT_PASSWORD", "romm-root-password")
                .withEnv("MARIADB_DATABASE", DB_NAME)
                .withEnv("MARIADB_USER", DB_USER)
                .withEnv("MARIADB_PASSWORD", DB_PASSWORD)
                // The entrypoint starts a temporary server to run the init, then the real one: ready twice.
                .waitingFor(
                    Wait
                        .forLogMessage(".*mariadbd: ready for connections.*\\n", 2)
                        .withStartupTimeout(DATABASE_START_TIMEOUT),
                )

        private fun rommContainer(
            network: Network,
            libraryDir: Path,
        ): GenericContainer<*> =
            GenericContainer(DockerImageName.parse(ROMM_IMAGE))
                .withNetwork(network)
                .withEnv("DB_HOST", DB_ALIAS)
                .withEnv("DB_NAME", DB_NAME)
                .withEnv("DB_USER", DB_USER)
                .withEnv("DB_PASSWD", DB_PASSWORD)
                .withEnv("ROMM_AUTH_SECRET_KEY", AUTH_SECRET_KEY)
                .withCopyFileToContainer(MountableFile.forHostPath(libraryDir), "/romm/library")
                .withExposedPorts(ROMM_PORT)
                .waitingFor(
                    Wait
                        .forHttp("/api/heartbeat")
                        .forPort(ROMM_PORT)
                        .forStatusCode(HTTP_OK)
                        .withStartupTimeout(ROMM_START_TIMEOUT),
                )
    }
}
