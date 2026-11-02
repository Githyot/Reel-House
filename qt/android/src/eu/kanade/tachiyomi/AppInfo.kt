// Adapted from Mihon (https://github.com/mihonapp/mihon, commit e588e043), Apache License 2.0.
// See eu/kanade/tachiyomi/NOTICE-mihon.md.

package eu.kanade.tachiyomi

/**
 * Used by extensions.
 *
 * Mihon reads its BuildConfig; here the host fills [versionCode] and [versionName] from the package manager at start.
 */
@Suppress("UNUSED")
object AppInfo {
    internal var versionCode: Int = 0

    internal var versionName: String = ""

    /**
     * Version code of the host application. May be useful for sharing as User-Agent information.
     * Note that this value differs between forks so logic should not rely on it.
     *
     * @since extension-lib 1.3
     */
    fun getVersionCode(): Int = versionCode

    /**
     * Version name of the host application. May be useful for sharing as User-Agent information.
     * Note that this value differs between forks so logic should not rely on it.
     *
     * @since extension-lib 1.3
     */
    fun getVersionName(): String = versionName

    /**
     * A list of supported image MIME types by the reader.
     * e.g. ["image/jpeg", "image/png", ...]
     *
     * @since extension-lib 1.5
     */
    fun getSupportedImageMimeTypes(): List<String> = listOf(
        "image/avif", "image/gif", "image/heif", "image/jpeg", "image/jxl", "image/png", "image/webp",
    )
}
