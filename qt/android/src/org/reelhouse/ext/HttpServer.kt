package org.reelhouse.ext

import android.util.Log
import java.io.BufferedInputStream
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.io.OutputStream
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.SocketException
import java.net.URLDecoder
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlin.concurrent.thread

internal class HttpRequest(
    val method: String,
    val path: String,
    val query: Map<String, String>,
    val body: ByteArray,
)

/**
 * [length] -1 means unknown; the server then buffers what [write] produces to send a Content-Length.
 * [close] runs once the reply is done with, sent or not, to release what [write] would have read from.
 */
internal class HttpReply(
    val status: Int,
    val contentType: String,
    val length: Long,
    val close: () -> Unit = {},
    val write: (OutputStream) -> Unit,
) {
    companion object {
        fun bytes(status: Int, contentType: String, data: ByteArray) =
            HttpReply(status, contentType, data.size.toLong()) { it.write(data) }

        fun text(status: Int, message: String) =
            bytes(status, "text/plain; charset=utf-8", message.toByteArray())
    }
}

/**
 * A deliberately small HTTP/1.1 server for the core on the same device: one request per connection (always
 * `Connection: close`, so no keep-alive bookkeeping), bodies only with Content-Length, bound to 127.0.0.1 only.
 */
internal class HttpServer(private val handler: (HttpRequest) -> HttpReply) {

    private val server = ServerSocket(0, 64, InetAddress.getByName("127.0.0.1"))

    val port: Int get() = server.localPort

    // Source calls block for seconds while waiting on websites, so many may run at once; idle threads go away
    private val pool = ThreadPoolExecutor(
        MAX_THREADS, MAX_THREADS, 30, TimeUnit.SECONDS, LinkedBlockingQueue(),
        object : java.util.concurrent.ThreadFactory {
            private val count = AtomicInteger()
            override fun newThread(r: Runnable) =
                Thread(r, "ext-http-${count.incrementAndGet()}").apply { isDaemon = true }
        },
    ).apply { allowCoreThreadTimeOut(true) }

    init {
        thread(name = "ext-http-accept", isDaemon = true) {
            while (!server.isClosed) {
                val socket = try {
                    server.accept()
                } catch (e: SocketException) {
                    break
                }
                pool.execute { serve(socket) }
            }
        }
    }

    private fun serve(socket: Socket) {
        socket.use {
            try {
                it.soTimeout = READ_TIMEOUT_MS
                val input = BufferedInputStream(it.getInputStream())
                val output = it.getOutputStream().buffered()
                val reply = try {
                    readRequest(input)?.let(handler) ?: HttpReply.text(400, "Bad request")
                } catch (e: TooLarge) {
                    HttpReply.text(413, "Request too large")
                } catch (e: Throwable) {
                    Log.e(TAG, "Request failed", e)
                    HttpReply.text(500, e.message ?: e.javaClass.simpleName)
                }
                try {
                    send(output, reply)
                } finally {
                    reply.close()
                }
            } catch (e: Throwable) {
                // The client went away (a scrolled-past cover, a closed page): nothing to answer
                Log.d(TAG, "Connection dropped: $e")
            }
        }
    }

    private fun readRequest(input: InputStream): HttpRequest? {
        val head = readHead(input) ?: return null
        val lines = head.split("\r\n")
        val parts = lines.first().split(' ')
        if (parts.size < 3) return null
        val headers = lines.drop(1).mapNotNull { line ->
            val colon = line.indexOf(':')
            if (colon <= 0) null else line.substring(0, colon).trim().lowercase() to line.substring(colon + 1).trim()
        }.toMap()

        if (headers["transfer-encoding"] != null) return null
        val length = headers["content-length"]?.toIntOrNull() ?: 0
        if (length > MAX_BODY) throw TooLarge()
        val body = ByteArray(length)
        var read = 0
        while (read < length) {
            val n = input.read(body, read, length - read)
            if (n < 0) return null
            read += n
        }

        val target = parts[1]
        val path = target.substringBefore('?')
        val query = target.substringAfter('?', "").split('&').filter { it.isNotEmpty() }.associate { pair ->
            decode(pair.substringBefore('=')) to decode(pair.substringAfter('=', ""))
        }
        return HttpRequest(parts[0].uppercase(), path, query, body)
    }

    /** Reads up to the blank line that ends the headers; null when the client closed early. */
    private fun readHead(input: InputStream): String? {
        val buffer = ByteArrayOutputStream()
        var matched = 0 // how much of "\r\n\r\n" was just seen
        while (true) {
            val b = input.read()
            if (b < 0) return null
            buffer.write(b)
            if (buffer.size() > MAX_HEAD) throw TooLarge()
            matched = when {
                b == '\r'.code && (matched == 0 || matched == 2) -> matched + 1
                b == '\n'.code && (matched == 1 || matched == 3) -> matched + 1
                b == '\r'.code -> 1
                else -> 0
            }
            if (matched == 4) return buffer.toString("ISO-8859-1").trimEnd()
        }
    }

    private fun send(output: OutputStream, reply: HttpReply) {
        var length = reply.length
        var writer = reply.write
        if (length < 0) {
            val data = ByteArrayOutputStream().also(reply.write).toByteArray()
            length = data.size.toLong()
            writer = { it.write(data) }
        }
        val head = "HTTP/1.1 ${reply.status} ${reason(reply.status)}\r\n" +
            "Content-Type: ${reply.contentType}\r\n" +
            "Content-Length: $length\r\n" +
            "Connection: close\r\n\r\n"
        output.write(head.toByteArray(Charsets.ISO_8859_1))
        writer(output)
        output.flush()
    }

    private class TooLarge : Exception()

    private companion object {
        const val TAG = "ReelHouseExt"
        const val MAX_THREADS = 32
        const val READ_TIMEOUT_MS = 30_000
        const val MAX_HEAD = 64 * 1024
        const val MAX_BODY = 4 * 1024 * 1024

        fun decode(s: String): String = URLDecoder.decode(s, "UTF-8")

        fun reason(status: Int) = when (status) {
            200 -> "OK"
            400 -> "Bad Request"
            404 -> "Not Found"
            405 -> "Method Not Allowed"
            413 -> "Payload Too Large"
            502 -> "Bad Gateway"
            else -> "Internal Server Error"
        }
    }
}
