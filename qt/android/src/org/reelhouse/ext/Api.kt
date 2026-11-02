package org.reelhouse.ext

import android.util.Log
import eu.kanade.tachiyomi.network.GET
import eu.kanade.tachiyomi.network.NetworkHelper
import eu.kanade.tachiyomi.network.awaitSuccess
import eu.kanade.tachiyomi.source.CatalogueSource
import eu.kanade.tachiyomi.source.Source
import eu.kanade.tachiyomi.source.model.MangasPage
import eu.kanade.tachiyomi.source.model.Page
import eu.kanade.tachiyomi.source.model.SChapter
import eu.kanade.tachiyomi.source.model.SManga
import eu.kanade.tachiyomi.source.online.HttpSource
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import okhttp3.Response
import uy.kohesive.injekt.Injekt
import uy.kohesive.injekt.api.get
import java.io.IOException
import java.net.URLEncoder

/** The operations of POST /api and GET /image (docs/android-extensions.md). */
internal class Api(private val loader: ExtensionLoader, private val port: Int) {

    // Calls run here so a request can give up on a stuck source without waiting for it (see [call])
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    fun handle(request: HttpRequest): HttpReply = when {
        request.path == "/api" && request.method == "POST" -> {
            val answer = try {
                op(Json.parseToJsonElement(request.body.decodeToString()).jsonObject)
            } catch (e: Throwable) {
                Log.w(TAG, "api failed", e)
                buildJsonObject { put("error", describe(e)) }
            }
            HttpReply.bytes(200, "application/json", answer.toString().toByteArray())
        }
        request.path == "/image" && request.method == "GET" -> try {
            image(request.query)
        } catch (e: Throwable) {
            Log.w(TAG, "image failed: ${request.query["url"]}", e)
            HttpReply.text(502, describe(e))
        }
        request.path == "/api" || request.path == "/image" -> HttpReply.text(405, "Method not allowed")
        else -> HttpReply.text(404, "Not found")
    }

    @Suppress("DEPRECATION")
    private fun op(req: JsonObject): JsonObject {
        fun str(key: String) = req[key]?.jsonPrimitive?.content ?: throw IllegalArgumentException("Missing \"$key\"")
        val page by lazy { req["page"]?.jsonPrimitive?.intOrNull ?: 1 }
        val source by lazy { sourceOf(str("source")) }
        val catalogue by lazy {
            source as? CatalogueSource ?: throw IllegalArgumentException("${source.name} has no catalogue")
        }

        loader.awaitReady()
        return when (val op = str("op")) {
            "installed" -> buildJsonObject { put("extensions", JsonArray(loader.all.map(::extJson))) }
            "install" -> extJson(loader.install(str("apk")))
            "uninstall" -> {
                loader.uninstall(str("pkg"))
                JsonObject(emptyMap())
            }
            "sources" -> buildJsonObject {
                put("sources", JsonArray(loader.all.flatMap { ext -> ext.sources.map { srcJson(it, ext.pkg) } }))
            }
            "popular" -> pageJson(source, call { catalogue.getPopularManga(page) })
            "latest" -> {
                require(source.supportsLatest) { "${source.name} has no latest updates" }
                pageJson(source, call { catalogue.getLatestUpdates(page) })
            }
            "search" -> {
                val query = str("query")
                pageJson(source, call { catalogue.getSearchManga(page, query, catalogue.getFilterList()) })
            }
            "details" -> {
                val manga = newManga(str("url"))
                val update = call { source.getMangaUpdate(manga, emptyList(), fetchDetails = true, fetchChapters = false) }
                mangaJson(source, update.manga, manga.url)
            }
            "chapters" -> {
                val manga = newManga(str("url"))
                val update = call { source.getMangaUpdate(manga, emptyList(), fetchDetails = false, fetchChapters = true) }
                val chapters = update.chapters.mapNotNull { chapter ->
                    // Old extensions still fix numbers and names here; Mihon calls it too
                    (source as? HttpSource)?.prepareNewChapter(chapter, manga)
                    chapterJson(chapter)
                }
                buildJsonObject { put("chapters", JsonArray(chapters)) }
            }
            "pages" -> {
                val chapter = SChapter.create().apply {
                    url = str("url")
                    name = ""
                }
                val pages = call { source.getPageList(chapter) }
                buildJsonObject {
                    put("pages", JsonArray(pages.mapIndexed { i, p -> JsonPrimitive(pageAddress(source, p, i)) }))
                }
            }
            else -> throw IllegalArgumentException("Unknown op \"$op\"")
        }
    }

    /**
     * GET /image. Covers (no `index`) are fetched like Mihon's cover loader: the source's client and headers.
     * Reader pages (`index`, from [pageAddress]) go through the source's own `imageRequest`, which often adds a
     * Referer from the page url (`ref`); with `page=1` the picture's address is asked of the source first.
     */
    private fun image(query: Map<String, String>): HttpReply {
        val url = query["url"] ?: throw IllegalArgumentException("Missing url")
        val source = sourceOf(query["source"] ?: throw IllegalArgumentException("Missing source"))
        val index = query["index"]?.toIntOrNull()

        val response: Response = call(IMAGE_TIMEOUT_MS) {
            when {
                source !is HttpSource -> Injekt.get<NetworkHelper>().client.newCall(GET(url)).awaitSuccess()
                query["page"] == "1" -> {
                    val page = Page(index ?: 0, url)
                    page.imageUrl = source.getImageUrl(page)
                    source.getImage(page)
                }
                index != null -> source.getImage(Page(index, query["ref"].orEmpty(), url))
                else -> source.client.newCall(GET(url, source.headers)).awaitSuccess()
            }
        }
        val body = response.body
        return HttpReply(
            status = 200,
            contentType = body.contentType()?.toString() ?: "application/octet-stream",
            length = body.contentLength(),
            close = { response.close() },
        ) { out -> body.byteStream().copyTo(out) }
    }

    /**
     * Runs a source call off this thread and waits at most [timeoutMs]. Rx-based sources block inside the call, so
     * cancelling cannot interrupt them; on timeout the request answers at once and the call finishes on its own
     * (OkHttp's call timeout bounds it).
     */
    private fun <T> call(timeoutMs: Long = API_TIMEOUT_MS, block: suspend () -> T): T {
        val job = scope.async { block() }
        return runBlocking {
            try {
                withTimeout(timeoutMs) { job.await() }
            } catch (e: TimeoutCancellationException) {
                job.cancel()
                throw IOException("The source did not answer within ${timeoutMs / 1000} s")
            }
        }
    }

    private fun sourceOf(id: String): Source {
        val key = id.toLongOrNull() ?: throw IllegalArgumentException("Bad source id \"$id\"")
        return loader.source(key) ?: throw IllegalArgumentException("Unknown source $id")
    }

    private fun newManga(url: String) = SManga.create().apply {
        this.url = url
        title = ""
    }

    private fun extJson(ext: LoadedExtension) = buildJsonObject {
        put("pkg", ext.pkg)
        put("name", ext.name)
        put("version", ext.versionName)
        put("code", ext.versionCode)
        put("lang", ext.lang)
        put("nsfw", ext.nsfw)
        put("sources", JsonArray(ext.sources.map { srcJson(it, ext.pkg) }))
    }

    private fun srcJson(source: Source, pkg: String) = buildJsonObject {
        put("id", source.id.toString())
        put("name", source.name)
        put("lang", source.lang)
        put("supportsLatest", source.supportsLatest)
        put("pkg", pkg)
    }

    private fun pageJson(source: Source, page: MangasPage) = buildJsonObject {
        put("mangas", JsonArray(page.mangas.mapNotNull { manga -> safe { manga.url }?.let { mangaJson(source, manga, it) } }))
        put("hasNext", page.hasNextPage)
    }

    // Parsers often build a fresh SManga without url or title (lateinit), so every read is guarded
    private fun mangaJson(source: Source, manga: SManga, fallbackUrl: String) = buildJsonObject {
        put("url", safe { manga.url } ?: fallbackUrl)
        put("title", safe { manga.title }.orEmpty())
        put("thumbnail", manga.thumbnail_url?.takeIf { it.isNotBlank() }?.let { imageAddress(source, it) }.orEmpty())
        put("author", manga.author.orEmpty())
        put("artist", manga.artist.orEmpty())
        put("description", manga.description.orEmpty())
        put("genre", manga.genre.orEmpty())
        put("status", manga.status)
        put("initialized", manga.initialized)
    }

    private fun chapterJson(chapter: SChapter): JsonObject? {
        val url = safe { chapter.url } ?: return null
        return buildJsonObject {
            put("url", url)
            put("name", safe { chapter.name }.orEmpty())
            put("date", chapter.date_upload)
            put("number", chapter.chapter_number)
            put("scanlator", chapter.scanlator.orEmpty())
        }
    }

    private fun pageAddress(source: Source, page: Page, index: Int): String {
        val imageUrl = page.imageUrl
        return if (!imageUrl.isNullOrBlank()) {
            imageAddress(source, imageUrl) + "&index=$index" +
                if (page.url.isNotEmpty()) "&ref=${encode(page.url)}" else ""
        } else {
            imageAddress(source, page.url) + "&page=1&index=$index"
        }
    }

    private fun imageAddress(source: Source, url: String) =
        "http://127.0.0.1:$port/image?source=${source.id}&url=${encode(url)}"

    private companion object {
        const val TAG = "ReelHouseExt"
        const val API_TIMEOUT_MS = 90_000L
        const val IMAGE_TIMEOUT_MS = 120_000L

        fun encode(s: String): String = URLEncoder.encode(s, "UTF-8")

        inline fun <T> safe(get: () -> T): T? = try {
            get()
        } catch (_: UninitializedPropertyAccessException) {
            null
        }

        fun describe(e: Throwable): String {
            var cause = e
            while (cause.cause != null && cause.cause !== cause && cause.message.isNullOrBlank()) cause = cause.cause!!
            return cause.message?.takeIf { it.isNotBlank() } ?: cause.javaClass.simpleName
        }
    }
}
