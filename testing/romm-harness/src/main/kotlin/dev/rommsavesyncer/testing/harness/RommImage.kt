package dev.rommsavesyncer.testing.harness

/** The RomM image the integration tests run against. */
const val ROMM_IMAGE = "rommapp/romm:5.4.1"

/** The database RomM runs on; pinned like [ROMM_IMAGE] (the docker-compose.yml uses the same tag). */
const val MARIADB_IMAGE = "mariadb:11.8.9"
