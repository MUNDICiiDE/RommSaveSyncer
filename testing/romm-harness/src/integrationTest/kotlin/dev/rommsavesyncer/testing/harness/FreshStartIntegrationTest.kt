package dev.rommsavesyncer.testing.harness

import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/** Starts its own stacks (not the shared one) so it can leave state behind and then look for it. */
class FreshStartIntegrationTest {
    @Test
    fun aSecondStackStartsCleanAfterTheFirstLeftStateBehind() {
        RommStack.start().use { first ->
            val env = RommBootstrap.bootstrap(first.baseUrl)
            val extraUser =
                buildJsonObject {
                    put("username", "leftover")
                    put("email", "leftover@example.com")
                    put("password", "leftover-password")
                    put("role", "user")
                }
            env.admin.postJson("/api/users", extraUser).requireSuccess("Creating a leftover user")
            assertEquals(listOf("admin", "leftover"), usernames(env))
        }

        RommStack.start().use { second ->
            val system =
                RommBootstrap
                    .heartbeat(second.baseUrl)
                    .jsonObject()
                    .getValue("SYSTEM")
                    .jsonObject
            assertTrue(
                system
                    .getValue("SHOW_SETUP_WIZARD")
                    .jsonPrimitive.content
                    .toBoolean(),
                "A clean server has no admin yet",
            )

            // Only succeeds when the first run's admin is gone: RomM refuses a second "first" user.
            val env = RommBootstrap.bootstrap(second.baseUrl)

            assertEquals(listOf("admin"), usernames(env))
            assertTrue(
                env.admin
                    .get("/api/saves")
                    .json()
                    .jsonArray
                    .isEmpty(),
            )
            assertTrue(
                env.admin
                    .get("/api/devices")
                    .json()
                    .jsonArray
                    .isEmpty(),
            )
        }
    }

    private fun usernames(env: RommEnvironment): List<String> =
        env.admin
            .get("/api/users")
            .requireSuccess("Listing users")
            .json()
            .jsonArray
            .map {
                it.jsonObject
                    .getValue("username")
                    .jsonPrimitive.content
            }.sorted()
}
