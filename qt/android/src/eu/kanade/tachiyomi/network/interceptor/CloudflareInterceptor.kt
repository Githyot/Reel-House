// Adapted from Mihon (https://github.com/mihonapp/mihon, commit e588e043), Apache License 2.0.
// See eu/kanade/tachiyomi/NOTICE-mihon.md.

package eu.kanade.tachiyomi.network.interceptor

import okhttp3.Interceptor
import okhttp3.Response
import java.io.IOException

/**
 * Sits in the default client where Mihon keeps its Cloudflare interceptor; newer extensions refuse to start without
 * one there. Mihon solves the challenge in a hidden WebView; this one does not yet, so a challenged page fails with a
 * clear message instead of the site's HTML.
 */
class CloudflareInterceptor : Interceptor {

    override fun intercept(chain: Interceptor.Chain): Response {
        val response = chain.proceed(chain.request())
        if (response.code in ERROR_CODES && response.header("Server") in SERVER_CHECK) {
            response.close()
            throw IOException("This site asks for a Cloudflare check, which the app cannot pass yet.")
        }
        return response
    }

    private companion object {
        val ERROR_CODES = listOf(403, 503)
        val SERVER_CHECK = arrayOf("cloudflare-nginx", "cloudflare")
    }
}
