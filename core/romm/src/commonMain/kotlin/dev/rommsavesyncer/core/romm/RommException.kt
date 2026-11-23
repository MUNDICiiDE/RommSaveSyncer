package dev.rommsavesyncer.core.romm

/**
 * Everything that can go wrong talking to RomM, as one closed set the engine can `when` over.
 * HTTP failures carry RomM's own `detail` text; [Network] covers requests that never got an answer.
 */
sealed class RommException(
    message: String,
    cause: Throwable? = null,
) : Exception(message, cause) {
    /** 401: the token is missing, expired, revoked or unknown. The caller must sign in again. */
    class Unauthorized(
        detail: String,
    ) : RommException("RomM rejected the credentials: $detail")

    /** 403: the token is valid but lacks the scope the endpoint needs. */
    class Forbidden(
        detail: String,
    ) : RommException("RomM refused the request: $detail")

    /** 404: the save, device, session or ROM does not exist (for this user). */
    class NotFound(
        detail: String,
    ) : RommException("Not found on RomM: $detail")

    /**
     * 409: RomM's upload guard. The slot has a newer save than this device last synced, so the upload was
     * refused rather than overwriting it. Resolve (download or ask the user), then upload with `overwrite`.
     */
    class Conflict(
        val detail: String,
    ) : RommException("RomM reports a conflict: $detail")

    /** 400 or 422: RomM rejected the request itself. */
    class InvalidRequest(
        val status: Int,
        val detail: String,
    ) : RommException("RomM rejected the request (HTTP $status): $detail")

    /** 429: too many requests; try again later. */
    class RateLimited(
        detail: String,
    ) : RommException("RomM is rate limiting this client: $detail")

    /** 5xx: RomM failed to handle a valid request. */
    class ServerError(
        val status: Int,
        val detail: String,
    ) : RommException("RomM failed (HTTP $status): $detail")

    /** A status the client does not expect for the call that was made. */
    class UnexpectedResponse(
        val status: Int,
        val detail: String,
    ) : RommException("Unexpected response from RomM (HTTP $status): $detail")

    /** The request never completed: no connection, a timeout or a dropped transfer. RomM may be offline. */
    class Network(
        cause: Throwable,
    ) : RommException("Could not reach RomM: ${cause.message}", cause)

    /** The server is older than [MINIMUM_ROMM_VERSION], or did not report a version. */
    class UnsupportedServer(
        val reportedVersion: String,
    ) : RommException("RomM $reportedVersion is not supported; $MINIMUM_ROMM_VERSION or newer is required")

    /** The device code expired before the user approved it (RFC 8628 `expired_token`). */
    class DeviceCodeExpired : RommException("The device code expired before it was approved")

    /** The user denied the device on RomM (RFC 8628 `access_denied`). */
    class DeviceAccessDenied : RommException("The device was denied on RomM")
}
