// Adapted from Mihon (https://github.com/mihonapp/mihon, commit e588e043), Apache License 2.0.
// See eu/kanade/tachiyomi/NOTICE-mihon.md.

package eu.kanade.tachiyomi.network

interface ProgressListener {
    fun update(bytesRead: Long, contentLength: Long, done: Boolean)
}
