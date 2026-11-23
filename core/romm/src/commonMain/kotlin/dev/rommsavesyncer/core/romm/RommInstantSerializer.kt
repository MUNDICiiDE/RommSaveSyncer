package dev.rommsavesyncer.core.romm

import kotlinx.serialization.KSerializer
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlin.time.Instant

private const val NANOS_PER_MICRO = 1_000

/**
 * RomM writes timestamps as ISO 8601 with an offset (`2026-11-23T10:00:00+00:00`), except play sessions, which
 * can come back without one; those are UTC. We send `Z`.
 */
object RommInstantSerializer : KSerializer<Instant> {
    override val descriptor = PrimitiveSerialDescriptor("RommInstant", PrimitiveKind.STRING)

    override fun serialize(
        encoder: Encoder,
        value: Instant,
    ) = encoder.encodeString(toMicroseconds(value).toString())

    override fun deserialize(decoder: Decoder): Instant {
        val text = decoder.decodeString()
        return Instant.parse(if (hasOffset(text)) text else "${text}Z")
    }

    /** Python's datetime keeps microseconds, so finer digits are cut before they reach RomM. */
    private fun toMicroseconds(value: Instant): Instant =
        Instant.fromEpochSeconds(value.epochSeconds, value.nanosecondsOfSecond / NANOS_PER_MICRO * NANOS_PER_MICRO)

    private fun hasOffset(text: String): Boolean {
        val time = text.substringAfter('T', "")
        return time.endsWith('Z', ignoreCase = true) || '+' in time || '-' in time
    }
}
