package dev.rommsavesyncer.testing.harness

import org.junit.jupiter.api.extension.ExtendWith

/**
 * Gives a test class a running, bootstrapped RomM server: declare a [RommEnvironment] parameter on a test
 * method or constructor. The stack starts on first use and is shared by every test class in the JVM; it is
 * removed, with its data, when the test run ends. Needs a running Docker daemon.
 */
@Target(AnnotationTarget.CLASS)
@Retention(AnnotationRetention.RUNTIME)
@ExtendWith(RommServerExtension::class)
annotation class RommServer
