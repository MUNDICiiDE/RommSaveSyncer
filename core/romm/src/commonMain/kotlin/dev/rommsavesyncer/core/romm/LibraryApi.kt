package dev.rommsavesyncer.core.romm

import io.ktor.client.request.parameter
import io.ktor.http.HttpMethod
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** The part of RomM's ROM record (`SimpleRomSchema`) the app uses. */
@Serializable
data class RommRom(
    val id: Int,
    val name: String? = null,
    @SerialName("fs_name") val fileName: String,
    @SerialName("fs_size_bytes") val fileSizeBytes: Long = 0,
    @SerialName("platform_id") val platformId: Int,
    @SerialName("platform_slug") val platformSlug: String,
    @SerialName("md5_hash") val md5: String? = null,
    @SerialName("sha1_hash") val sha1: String? = null,
    @SerialName("crc_hash") val crc: String? = null,
)

/** One page of `GET /api/roms`; [total] is the size of the whole result. */
@Serializable
data class RomPage(
    val items: List<RommRom>,
    val total: Int? = null,
    val limit: Int,
    val offset: Int,
)

interface RommLibraryApi {
    /** `GET /api/roms` (scope `roms.read`), filtered by a name search and/or a platform. */
    suspend fun listRoms(
        searchTerm: String? = null,
        platformId: Int? = null,
        limit: Int = DEFAULT_PAGE_SIZE,
        offset: Int = 0,
    ): RomPage

    /**
     * `GET /api/roms/by-hash` (scope `roms.read`): the ROM with this file hash, or null when RomM has none.
     * At least one hash is required.
     */
    suspend fun findRomByHash(
        md5: String? = null,
        sha1: String? = null,
        crc: String? = null,
    ): RommRom?

    companion object {
        const val DEFAULT_PAGE_SIZE = 50
    }
}

internal class KtorLibraryApi(
    private val transport: RommTransport,
) : RommLibraryApi {
    override suspend fun listRoms(
        searchTerm: String?,
        platformId: Int?,
        limit: Int,
        offset: Int,
    ): RomPage {
        val response =
            transport.send(HttpMethod.Get, "/api/roms") {
                parameter("search_term", searchTerm)
                parameter("platform_ids", platformId)
                parameter("limit", limit)
                parameter("offset", offset)
                // The indexes and filter lists the web UI needs are costly and of no use here.
                parameter("with_char_index", false)
                parameter("with_filter_values", false)
                parameter("with_rom_id_index", false)
            }
        return transport.decode(response)
    }

    override suspend fun findRomByHash(
        md5: String?,
        sha1: String?,
        crc: String?,
    ): RommRom? {
        require(md5 != null || sha1 != null || crc != null) { "At least one hash is required" }
        return try {
            val response =
                transport.send(HttpMethod.Get, "/api/roms/by-hash") {
                    parameter("md5_hash", md5)
                    parameter("sha1_hash", sha1)
                    parameter("crc_hash", crc)
                }
            transport.decode<RommRom>(response)
        } catch (ignored: RommException.NotFound) {
            null
        }
    }
}
