package org.reelhouse.ext

import android.app.Application
import android.content.Context
import android.util.Log
import eu.kanade.tachiyomi.AppInfo
import eu.kanade.tachiyomi.network.JavaScriptEngine
import eu.kanade.tachiyomi.network.NetworkHelper
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.json.Json
import kotlinx.serialization.protobuf.ProtoBuf
import uy.kohesive.injekt.Injekt
import uy.kohesive.injekt.api.addSingleton
import uy.kohesive.injekt.api.addSingletonFactory
import java.io.File
import kotlin.concurrent.thread

/**
 * Entry point called from C++ (main.cpp) once the window exists. See docs/android-extensions.md for the protocol.
 */
object ExtensionHost {

    private const val TAG = "ReelHouseExt"

    private var port = 0

    /** Starts the local server and loads the installed extensions; returns the port, or 0 on failure. */
    @JvmStatic
    @Synchronized
    fun start(context: Context, dir: String): Int {
        if (port != 0) return port
        return try {
            val app = context.applicationContext as Application
            registerInjekt(app)
            fillAppInfo(app)

            val loader = ExtensionLoader(app, File(dir).apply { mkdirs() })
            lateinit var api: Api
            val server = HttpServer { api.handle(it) }
            api = Api(loader, server.port)
            // Loading dex files takes a while; requests wait for it instead of holding up the caller
            thread(name = "ext-load", isDaemon = true) { loader.loadInstalled() }

            port = server.port
            Log.i(TAG, "Extension host on 127.0.0.1:$port")
            port
        } catch (e: Throwable) {
            Log.e(TAG, "Extension host failed to start", e)
            0
        }
    }

    /**
     * What extensions take from Injekt (`injectLazy()` / `Injekt.get()`), the same set Mihon exposes in its
     * MetroInjektRegistrar except kotlinx XML, which this app does not ship.
     */
    @OptIn(ExperimentalSerializationApi::class)
    private fun registerInjekt(app: Application) {
        Injekt.addSingleton<Application>(app)
        Injekt.addSingleton<Context>(app)
        Injekt.addSingletonFactory<NetworkHelper> { NetworkHelper(app) }
        Injekt.addSingletonFactory<JavaScriptEngine> { JavaScriptEngine(app) }
        Injekt.addSingletonFactory<Json> {
            Json {
                ignoreUnknownKeys = true
                explicitNulls = false
            }
        }
        Injekt.addSingleton<ProtoBuf>(ProtoBuf)
    }

    private fun fillAppInfo(app: Application) {
        try {
            val info = app.packageManager.getPackageInfo(app.packageName, 0)
            AppInfo.versionCode = info.longVersionCode.toInt()
            AppInfo.versionName = info.versionName.orEmpty()
        } catch (e: Exception) {
            Log.w(TAG, "No package info: $e")
        }
    }
}
