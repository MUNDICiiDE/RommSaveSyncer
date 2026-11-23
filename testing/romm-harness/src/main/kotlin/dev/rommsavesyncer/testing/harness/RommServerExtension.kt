package dev.rommsavesyncer.testing.harness

import org.junit.jupiter.api.extension.ExtensionContext
import org.junit.jupiter.api.extension.ParameterContext
import org.junit.jupiter.api.extension.ParameterResolver

/** Resolves [RommEnvironment] parameters from one RomM stack per JVM, kept in the root store so JUnit closes it. */
class RommServerExtension : ParameterResolver {
    override fun supportsParameter(
        parameterContext: ParameterContext,
        extensionContext: ExtensionContext,
    ): Boolean = parameterContext.parameter.type == RommEnvironment::class.java

    override fun resolveParameter(
        parameterContext: ParameterContext,
        extensionContext: ExtensionContext,
    ): Any =
        extensionContext.root
            .getStore(NAMESPACE)
            .computeIfAbsent(SharedServer::class.java, { SharedServer() }, SharedServer::class.java)
            .environment

    private class SharedServer : AutoCloseable {
        private val stack = RommStack.start()
        val environment: RommEnvironment = bootstrapOrClose()

        private fun bootstrapOrClose(): RommEnvironment {
            var bootstrapped = false
            try {
                return RommBootstrap.bootstrap(stack.baseUrl).also { bootstrapped = true }
            } finally {
                if (!bootstrapped) stack.close()
            }
        }

        override fun close() = stack.close()
    }

    private companion object {
        val NAMESPACE: ExtensionContext.Namespace = ExtensionContext.Namespace.create(RommServerExtension::class.java)
    }
}
