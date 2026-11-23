package dev.rommsavesyncer.core.romm

/**
 * Where the client gets the device token it sends as `Authorization: Bearer ...`.
 * The app's secret store implements this; the client never stores or refreshes tokens itself.
 */
interface RommTokenSource {
    /** The current token, or null when the device is not signed in. */
    suspend fun accessToken(): String?

    /**
     * Called when RomM answered 401 to [rejectedToken] (null when there was no token to send). The token is
     * expired, revoked or unknown, so the app should drop it and start the device flow again.
     */
    suspend fun onUnauthorized(rejectedToken: String?)
}

/** A fixed token, for tools and tests that never need to react to a 401. */
class StaticTokenSource(
    private val token: String?,
) : RommTokenSource {
    override suspend fun accessToken(): String? = token

    override suspend fun onUnauthorized(rejectedToken: String?) = Unit
}
