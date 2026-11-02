// Adapted from Mihon (https://github.com/mihonapp/mihon, commit e588e043), Apache License 2.0.
// See eu/kanade/tachiyomi/NOTICE-mihon.md.

package eu.kanade.tachiyomi.source

/**
 * A factory for creating sources at runtime.
 */
interface SourceFactory {
    /**
     * Create a new copy of the sources
     * @return The created sources
     */
    fun createSources(): List<Source>
}
