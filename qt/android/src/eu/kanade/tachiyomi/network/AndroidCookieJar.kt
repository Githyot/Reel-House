// Adapted from Mihon (https://github.com/mihonapp/mihon, commit e588e043), Apache License 2.0.
// See eu/kanade/tachiyomi/NOTICE-mihon.md.

package eu.kanade.tachiyomi.network

import android.webkit.CookieManager
import okhttp3.Cookie
import okhttp3.CookieJar
import okhttp3.HttpUrl
import java.util.concurrent.ConcurrentHashMap

/**
 * Cookies live in the WebView's [CookieManager], as in Mihon, so they survive restarts. Some Android TV boxes ship
 * without a WebView and `getInstance` throws there; those keep cookies in memory for the session instead.
 */
class AndroidCookieJar : CookieJar {

    private val manager: CookieManager? by lazy { runCatching { CookieManager.getInstance() }.getOrNull() }

    // Fallback store: host -> cookie name -> cookie
    private val memory = ConcurrentHashMap<String, ConcurrentHashMap<String, Cookie>>()

    override fun saveFromResponse(url: HttpUrl, cookies: List<Cookie>) {
        val manager = manager
        if (manager == null) {
            val forHost = memory.getOrPut(url.host) { ConcurrentHashMap() }
            cookies.forEach { forHost[it.name] = it }
            return
        }
        val urlString = url.toString()

        cookies.forEach { manager.setCookie(urlString, it.toString()) }
    }

    override fun loadForRequest(url: HttpUrl): List<Cookie> {
        return get(url)
    }

    fun get(url: HttpUrl): List<Cookie> {
        val manager = manager
        if (manager == null) {
            val now = System.currentTimeMillis()
            return memory[url.host]?.values?.filter { it.expiresAt > now && it.matches(url) }.orEmpty()
        }
        val cookies = manager.getCookie(url.toString())

        return if (cookies != null && cookies.isNotEmpty()) {
            cookies.split(";").mapNotNull { Cookie.parse(url, it) }
        } else {
            emptyList()
        }
    }

    fun remove(url: HttpUrl, cookieNames: List<String>? = null, maxAge: Int = -1): Int {
        val manager = manager
        if (manager == null) {
            val forHost = memory[url.host] ?: return 0
            val names = cookieNames ?: forHost.keys.toList()
            return names.count { forHost.remove(it) != null }
        }
        val urlString = url.toString()
        val cookies = manager.getCookie(urlString) ?: return 0

        fun List<String>.filterNames(): List<String> {
            return if (cookieNames != null) {
                this.filter { it in cookieNames }
            } else {
                this
            }
        }

        return cookies.split(";")
            .map { it.substringBefore("=") }
            .filterNames()
            .onEach { manager.setCookie(urlString, "$it=;Max-Age=$maxAge") }
            .count()
    }

    fun removeAll() {
        memory.clear()
        manager?.removeAllCookies {}
    }
}
