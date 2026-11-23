package dev.rommsavesyncer.core.romm

import io.ktor.client.HttpClient

/**
 * Everything the app asks of a RomM server, signed in with a device token. Every call throws a
 * [RommException] on failure; none of them retries.
 *
 * Verified against RomM 5.4.1 (`backend/endpoints`). Create one with [KtorRommClient].
 */
interface RommClient :
    RommServerApi,
    RommDeviceApi,
    RommSyncApi,
    RommSavesApi,
    RommPlaySessionsApi,
    RommLibraryApi

/**
 * The Ktor implementation of [RommClient]. [http] comes from [createRommHttpClient]; [baseUrl] is the server
 * root such as `https://romm.example.com`. [tokens] supplies the bearer token and hears about 401s.
 */
class KtorRommClient private constructor(
    transport: RommTransport,
) : RommClient,
    RommServerApi by KtorServerApi(transport),
    RommDeviceApi by KtorDeviceApi(transport),
    RommSyncApi by KtorSyncApi(transport),
    RommSavesApi by KtorSavesApi(transport),
    RommPlaySessionsApi by KtorPlaySessionsApi(transport),
    RommLibraryApi by KtorLibraryApi(transport) {
    constructor(
        http: HttpClient,
        baseUrl: String,
        tokens: RommTokenSource,
    ) : this(RommTransport(http, baseUrl, tokens))
}
