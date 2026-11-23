package dev.rommsavesyncer.core.romm

import io.ktor.client.engine.HttpClientEngine
import io.ktor.client.engine.okhttp.OkHttp

actual fun defaultRommEngine(): HttpClientEngine = OkHttp.create()
