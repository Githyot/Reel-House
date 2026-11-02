package org.reelhouse.ext

import android.content.Context
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.util.Log
import dalvik.system.DexClassLoader
import eu.kanade.tachiyomi.source.Source
import eu.kanade.tachiyomi.source.SourceFactory
import java.io.File
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

internal class LoadedExtension(
    val pkg: String,
    val name: String,
    val versionName: String,
    val versionCode: Long,
    val lang: String,
    val nsfw: Boolean,
    val sources: List<Source>,
)

/**
 * Loads Mihon/Tachiyomi extension APKs from `<dir>/installed` without installing them in the system, the way
 * Mihon's "private" extensions work (see its ExtensionLoader): read the manifest meta-data of the archive, open its
 * dex with a class loader whose parent is this app's (which carries the source API and the libraries), and
 * instantiate the classes named in `tachiyomi.extension.class`.
 */
internal class ExtensionLoader(private val context: Context, dir: File) {

    private val root = dir.canonicalFile
    private val installedDir = File(root, "installed")
    private val dexDir = File(root, "dex")

    private val extensions = ConcurrentHashMap<String, LoadedExtension>()
    private val ready = CountDownLatch(1)

    val all: List<LoadedExtension> get() = extensions.values.sortedBy { it.name.lowercase() }

    fun source(id: Long): Source? = extensions.values.firstNotNullOfOrNull { ext -> ext.sources.find { it.id == id } }

    fun pkgOf(source: Source): String = extensions.values.first { source in it.sources }.pkg

    /** Loads what was installed in earlier runs. Requests wait for it (see [awaitReady]). */
    fun loadInstalled() {
        try {
            installedDir.listFiles { f -> f.name.endsWith(".apk") }?.forEach { apk ->
                try {
                    load(apk).let { extensions[it.pkg] = it }
                } catch (e: Throwable) {
                    // Kept on disk: a newer app version may be able to load it, and the user can uninstall it
                    Log.e(TAG, "Could not load ${apk.name}", e)
                }
            }
        } finally {
            ready.countDown()
        }
    }

    fun awaitReady() {
        ready.await(2, TimeUnit.MINUTES)
    }

    @Synchronized
    fun install(apkPath: String): LoadedExtension {
        val apk = File(apkPath).canonicalFile
        // Anything on the device can reach the local port, so only APKs the core put in our private folder are loaded
        require(apk.path.startsWith(root.path + File.separator)) { "The APK must be inside ${root.path}" }
        require(apk.isFile) { "No file at $apkPath" }
        val pkg = readPackage(apk).packageName
        require(PKG_NAME.matches(pkg)) { "Unexpected package name $pkg" }

        installedDir.mkdirs()
        val target = File(installedDir, "$pkg.apk")
        val temp = File(installedDir, "$pkg.apk.tmp")
        temp.delete()
        apk.copyTo(temp)
        // Android 14 refuses to load dex from a writable file
        temp.setReadOnly()
        remove(target)
        check(temp.renameTo(target)) { "Could not move the APK into place" }

        return try {
            load(target).also { extensions[pkg] = it }
        } catch (e: Throwable) {
            extensions.remove(pkg)
            remove(target)
            throw e
        }
    }

    @Synchronized
    fun uninstall(pkg: String) {
        require(PKG_NAME.matches(pkg)) { "Unexpected package name $pkg" }
        // The classes stay in memory until the app restarts; nothing reaches them once the sources are gone
        extensions.remove(pkg)
        remove(File(installedDir, "$pkg.apk"))
    }

    private fun load(apk: File): LoadedExtension {
        val info = readPackage(apk)
        val pkg = info.packageName
        val appInfo = info.applicationInfo ?: error("$pkg has no application info")
        val meta = appInfo.metaData ?: error("$pkg has no meta-data")
        val versionName = info.versionName.orEmpty()

        val libVersion = meta.getFloat(METADATA_EXTENSION_LIB).takeUnless { it == 0f }?.toString()?.toDouble()
            ?: versionName.substringBeforeLast('.').toDoubleOrNull()
        require(libVersion != null && libVersion >= LIB_VERSION_MIN && libVersion <= LIB_VERSION_MAX) {
            "$pkg uses extension library $libVersion; supported: $LIB_VERSION_MIN to $LIB_VERSION_MAX"
        }

        val name = meta.getString(METADATA_NAME)
            ?: context.packageManager.getApplicationLabel(appInfo).toString().substringAfter("Tachiyomi: ")
        val nsfw = if (meta.containsKey(METADATA_CONTENT_WARNING)) {
            meta.getInt(METADATA_CONTENT_WARNING) == 2
        } else {
            meta.getInt(METADATA_NSFW) == 1
        }

        dexDir.mkdirs()
        val loader = ExtensionClassLoader(apk.path, dexDir.path, context.classLoader)
        val classNames = meta.getString(METADATA_SOURCE_CLASS)
        require(!classNames.isNullOrBlank()) { "$pkg names no source class" }
        val sources = classNames.split(";").map { it.trim() }.filter { it.isNotEmpty() }.flatMap { className ->
            val fullName = if (className.startsWith(".")) pkg + className else className
            when (val obj = Class.forName(fullName, false, loader).getDeclaredConstructor().newInstance()) {
                is Source -> listOf(obj)
                is SourceFactory -> obj.createSources()
                else -> error("Unknown source class type: ${obj.javaClass}")
            }
        }

        val langs = sources.map { it.lang }.toSet()
        return LoadedExtension(
            pkg = pkg,
            name = name,
            versionName = versionName,
            versionCode = info.longVersionCode,
            lang = when (langs.size) {
                0 -> ""
                1 -> langs.first()
                else -> "all"
            },
            nsfw = nsfw,
            sources = sources,
        )
    }

    private fun readPackage(apk: File): PackageInfo {
        @Suppress("DEPRECATION")
        val info = context.packageManager.getPackageArchiveInfo(
            apk.path,
            PackageManager.GET_META_DATA or PackageManager.GET_CONFIGURATIONS,
        ) ?: error("${apk.name} is not a readable APK")
        // Archives read this way have no paths, and the label and resources are read through them
        info.applicationInfo?.apply {
            sourceDir = apk.path
            publicSourceDir = apk.path
        }
        require(info.reqFeatures.orEmpty().any { it.name == EXTENSION_FEATURE }) {
            "${info.packageName} isn't a Tachiyomi extension"
        }
        return info
    }

    private fun remove(file: File) {
        if (file.exists()) {
            file.setWritable(true)
            file.delete()
        }
    }

    private companion object {
        const val TAG = "ReelHouseExt"

        const val EXTENSION_FEATURE = "tachiyomi.extension"
        const val METADATA_SOURCE_CLASS = "tachiyomi.extension.class"
        const val METADATA_NSFW = "tachiyomi.extension.nsfw"
        const val METADATA_NAME = "tachiyomix.name"
        const val METADATA_EXTENSION_LIB = "tachiyomix.extensionLib"
        const val METADATA_CONTENT_WARNING = "tachiyomix.contentWarning"

        // Mihon accepts 1.4 and 1.6 (1.5 was never released); the API here is Mihon's, so the same range
        const val LIB_VERSION_MIN = 1.4
        const val LIB_VERSION_MAX = 1.6

        val PKG_NAME = Regex("[A-Za-z0-9_]+(\\.[A-Za-z0-9_]+)*")
    }
}

/**
 * Looks classes up like Mihon's DelegateLastClassLoader: the boot class path first, then the extension's own dex,
 * then the app. An extension that bundles its own copy of a library then gets that copy, as it would in Mihon.
 * (`optimizedDirectory` is ignored since Android 8; ART keeps the compiled code next to the APK.)
 */
private class ExtensionClassLoader(dexPath: String, optimizedDir: String, parent: ClassLoader) :
    DexClassLoader(dexPath, optimizedDir, null, parent) {

    private val bootClassLoader: ClassLoader? = Any::class.java.classLoader

    override fun loadClass(name: String?, resolve: Boolean): Class<*> {
        findLoadedClass(name)?.let { return it }

        if (bootClassLoader != null) {
            try {
                return bootClassLoader.loadClass(name)
            } catch (_: ClassNotFoundException) {
            }
        }

        val fromDex = try {
            return findClass(name)
        } catch (e: ClassNotFoundException) {
            e
        }

        return try {
            parent.loadClass(name)
        } catch (_: ClassNotFoundException) {
            throw fromDex
        }
    }
}
