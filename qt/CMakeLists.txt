cmake_minimum_required(VERSION 3.21)
project(reelhouse_qt LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

# ---- where the app is built for ----
# Desktop (Windows through MSYS2, Linux, macOS) is the plain `cmake -S qt -B qt/build`. Android and iOS come through
# Qt's own kits (`<Qt>/<version>/android_arm64_v8a/bin/qt-cmake`, `<Qt>/<version>/ios/bin/qt-cmake -G Xcode`):
# build-android.sh and the workflows in .github/ show the whole run; docs/mobile-build.md explains it.
if(ANDROID OR IOS)
    set(REELHOUSE_MOBILE ON)
else()
    set(REELHOUSE_MOBILE OFF)
endif()
if(IOS)
    enable_language(OBJCXX)
endif()

find_package(Qt6 6.6 REQUIRED COMPONENTS Quick QuickControls2 Gui OpenGL Network Concurrent)
# Qt 6.8's OpenGL target still names the AGL framework on macOS, which the macOS 26 SDK (Xcode 26) no longer has:
# "ld: framework 'AGL' not found". Nothing of it is used, so it is dropped from the link.
if(APPLE AND NOT IOS AND TARGET WrapOpenGL::WrapOpenGL)
    get_target_property(_reelhouse_gl_libs WrapOpenGL::WrapOpenGL INTERFACE_LINK_LIBRARIES)
    if(_reelhouse_gl_libs)
        list(FILTER _reelhouse_gl_libs EXCLUDE REGEX "AGL")
        set_target_properties(WrapOpenGL::WrapOpenGL PROPERTIES INTERFACE_LINK_LIBRARIES "${_reelhouse_gl_libs}")
    endif()
endif()
# the microphone of Conversation (MicRecorder.qml + VoiceInput, voiceinput.cpp) and the offline voice (LocalTts.qml:
# Qt TextToSpeech, the system's voices: SAPI on Windows, speech-dispatcher on Linux); the page works without them.
# Conversation and the local AI are desktop only, so Android and iOS neither look for Qt Multimedia / Qt Speech nor
# pack KaiwaPage/MicRecorder/OllamaPanel/VoicePanel/VoiceHost/LocalTts.
if(NOT REELHOUSE_MOBILE)
    find_package(Qt6 6.6 QUIET OPTIONAL_COMPONENTS Multimedia TextToSpeech)
endif()

# ---- the Rust core (qtbridge/ -> libreelhouse_qtbridge.a) ----
set(RUST_DIR "${CMAKE_SOURCE_DIR}/..")
option(REELHOUSE_DEVPROXY "testing only: network through the HTTPS_PROXY of the environment" OFF)
# A script or the CI may build the core first (build-android.sh does): then only the library is linked.
option(REELHOUSE_BUILD_RUST "Build the Rust core from here (OFF when a script built it already)" ON)
set(REELHOUSE_RUST_LIB "" CACHE FILEPATH "libreelhouse_qtbridge.a built elsewhere (empty = the default place)")
set(RUST_FEATURES "")
if(REELHOUSE_DEVPROXY)
    set(RUST_FEATURES --features reelhouse/devproxy)
endif()
if(ANDROID)
    # cargo-ndk (cargo install cargo-ndk) points cargo at the NDK's clang and sysroot for the ABI. The `mobile` feature
    # leaves the helper programs out of the core (no rqbit, Suwayomi, upscaler, yt-dlp, Ollama on a phone), and
    # `js-bindgen` makes the QuickJS bindings on the build machine (libclang), since none are shipped for Android.
    if(CMAKE_ANDROID_ARCH_ABI STREQUAL "arm64-v8a")
        set(RUST_TRIPLE aarch64-linux-android)
    elseif(CMAKE_ANDROID_ARCH_ABI STREQUAL "x86_64")
        set(RUST_TRIPLE x86_64-linux-android)
    elseif(CMAKE_ANDROID_ARCH_ABI STREQUAL "armeabi-v7a")
        set(RUST_TRIPLE armv7-linux-androideabi)
    elseif(CMAKE_ANDROID_ARCH_ABI STREQUAL "x86")
        set(RUST_TRIPLE i686-linux-android)
    else()
        message(FATAL_ERROR "Android ABI ${CMAKE_ANDROID_ARCH_ABI}: no Rust target known for it")
    endif()
    set(RUST_TARGET_DIR "${RUST_DIR}/target-android")
    set(RUST_LIB_DEFAULT "${RUST_TARGET_DIR}/${RUST_TRIPLE}/release/libreelhouse_qtbridge.a")
    set(REELHOUSE_ANDROID_MIN_SDK 28)
    set(REELHOUSE_ANDROID_TARGET_SDK 34)
    set(_ndk "${ANDROID_NDK}")
    if(NOT _ndk AND DEFINED ENV{ANDROID_NDK_ROOT})
        set(_ndk "$ENV{ANDROID_NDK_ROOT}")
    endif()
    set(RUST_BUILD_CMD ${CMAKE_COMMAND} -E env "ANDROID_NDK_HOME=${_ndk}" "ANDROID_NDK_ROOT=${_ndk}"
        cargo ndk -t ${CMAKE_ANDROID_ARCH_ABI} --platform ${REELHOUSE_ANDROID_MIN_SDK} build --release
        --manifest-path "${RUST_DIR}/qtbridge/Cargo.toml" --target-dir "${RUST_TARGET_DIR}"
        --no-default-features --features "mobile,js-bindgen" ${RUST_FEATURES})
elseif(IOS)
    # Device builds are arm64 (aarch64-apple-ios); the simulator on Apple silicon is aarch64-apple-ios-sim.
    option(REELHOUSE_IOS_SIMULATOR "Build for the iOS simulator instead of a device" OFF)
    if(REELHOUSE_IOS_SIMULATOR OR CMAKE_OSX_SYSROOT MATCHES "[Ss]imulator")
        set(RUST_TRIPLE aarch64-apple-ios-sim)
    else()
        set(RUST_TRIPLE aarch64-apple-ios)
    endif()
    set(RUST_TARGET_DIR "${RUST_DIR}/target-ios")
    set(RUST_LIB_DEFAULT "${RUST_TARGET_DIR}/${RUST_TRIPLE}/release/libreelhouse_qtbridge.a")
    set(RUST_BUILD_CMD cargo build --release --target ${RUST_TRIPLE}
        --manifest-path "${RUST_DIR}/qtbridge/Cargo.toml" --target-dir "${RUST_TARGET_DIR}"
        --no-default-features --features "mobile,js-bindgen" ${RUST_FEATURES})
else()
    set(RUST_LIB_DEFAULT "${RUST_DIR}/target-qt/release/libreelhouse_qtbridge.a")
    set(RUST_BUILD_CMD cargo build --release --manifest-path "${RUST_DIR}/qtbridge/Cargo.toml" --target-dir "${RUST_DIR}/target-qt" ${RUST_FEATURES})
endif()
if(REELHOUSE_RUST_LIB)
    set(RUST_LIB "${REELHOUSE_RUST_LIB}")
else()
    set(RUST_LIB "${RUST_LIB_DEFAULT}")
endif()
if(REELHOUSE_BUILD_RUST)
    add_custom_target(rust_core ALL
        COMMAND ${RUST_BUILD_CMD}
        BYPRODUCTS "${RUST_LIB}"
        WORKING_DIRECTORY "${RUST_DIR}"
        COMMENT "Building the Rust core"
        USES_TERMINAL)
elseif(NOT EXISTS "${RUST_LIB}")
    message(FATAL_ERROR "REELHOUSE_BUILD_RUST is OFF and the Rust core is missing: ${RUST_LIB}")
endif()

qt_add_executable(reelhouse-qt
    src/main.cpp src/corebridge.cpp src/corebridge.h src/thememanager.cpp src/thememanager.h
    src/videoitem.cpp src/videoitem.h src/bridge.h src/imageprovider.cpp src/imageprovider.h src/editedpage.cpp src/editedpage.h src/richtext.cpp src/themevideo.cpp src/themevideo.h src/webpane.cpp src/webpane.h
    src/platform.cpp src/platform.h src/orbsitem.cpp src/orbsitem.h src/orbspaint.cpp src/orbspaint.h)
if(REELHOUSE_BUILD_RUST)
    add_dependencies(reelhouse-qt rust_core)
endif()
qt_add_resources(reelhouse-qt "app" PREFIX "/" FILES
    themes/gnome.json
    themes/apple.json
    themes/windows.json
    themes/android.json
    themes/ios.json
    themes/hyperos.json
    icon/reelhouse-icon.png
    icon/reelhouse-round-512.png
    qml/AboutDialog.qml
    qml/Accel.js
    qml/AndroidColorPicker.qml
    qml/AnkiPanel.qml
    qml/AnkiSharedBrowser.qml
    qml/AnkiWebLogin.qml
    qml/AppHeader.qml
    qml/AppMenu.qml
    qml/AppMenuItem.qml
    qml/AppleColorPicker.qml
    qml/ArtDialog.qml
    qml/Aurora.qml
    qml/BannerHost.qml
    qml/BellPopover.qml
    qml/BottomNav.qml
    qml/BubbleEditPanel.qml
    qml/BubbleText.qml
    qml/CalendarPage.qml
    qml/CatalogPopover.qml
    qml/ChannelRow.qml
    qml/Chip.qml
    qml/ColorMath.js
    qml/ColorPickerAdapter.qml
    qml/ColorPlane.qml
    qml/ColorSlider.qml
    qml/ContinueRow.qml
    qml/CoverGrid.qml
    qml/DetailPage.qml
    qml/DialogButtons.qml
    qml/DialogHeader.qml
    qml/DialogTitle.qml
    qml/DiscoverPage.qml
    qml/DlPickDialog.qml
    qml/FlatButton.qml
    qml/Fab.qml
    qml/FeedbackBox.qml
    qml/LoadingScreen.qml
    qml/OfflinePill.qml
    qml/GenreChip.qml
    qml/GenrePage.qml
    qml/Glass.qml
    qml/GnomeColorPicker.qml
    qml/HScrollList.qml
    qml/HeroBanner.qml
    qml/Icon.qml
    qml/ImagesPage.qml
    qml/InfoPage.qml
    qml/KanaChart.qml
    qml/KanaTrace.qml
    qml/KaraokeStudio.qml
    qml/LangButton.qml
    qml/Lbl.qml
    qml/LearnButton.qml
    qml/LearnGlyph.qml
    qml/LearnLesson.qml
    qml/LearnPage.qml
    qml/LearnSide.qml
    qml/LensProbe.qml
    qml/LibraryPage.qml
    qml/LucideIcons.js
    qml/MaterialIcons.js
    qml/LearnGlyphs.js
    qml/KanaHover.js
    qml/Main.qml
    qml/MangaReaderPage.qml
    qml/Mascot.qml
    qml/MascotStage.qml
    qml/ModeBrowser.qml
    qml/ModeButton.qml
    qml/NewBadge.qml
    qml/NovelReaderPage.qml
    qml/Osd.js
    qml/OsdButton.qml
    qml/OsdHeader.qml
    qml/OsdMenu.qml
    qml/OsdPanel.qml
    qml/OsdRow.qml
    qml/OsdSegmented.qml
    qml/OsdSlider.qml
    qml/OsdSwitch.qml
    qml/OsdWindowButtons.qml
    qml/UiSounds.qml
    qml/OrbsBackground.qml
    qml/PadBadge.qml
    qml/PadGlyph.qml
    qml/PadHints.qml
    qml/SoundVolumeBox.qml
    qml/PadKeyboard.qml
    qml/PrefsController.qml
    qml/PadNavigator.qml
    qml/PageCardPanel.qml
    qml/PageLookSheet.qml
    qml/PageTurn.qml
    qml/PaintTools.qml
    qml/PaperGrain.qml
    qml/PlayerPage.qml
    qml/PluginsDialog.qml
    qml/PomodoroButton.qml
    qml/PomodoroGlyph.qml
    qml/PomodoroWidget.qml
    qml/PopMenu.qml
    qml/Popover.qml
    qml/PosterCard.qml
    qml/PosterImage.qml
    qml/PosterRow.qml
    qml/PrefButton.qml
    qml/PrefCombo.qml
    qml/PrefEntry.qml
    qml/PrefGroup.qml
    qml/PrefKnownLangs.qml
    qml/PrefRow.qml
    qml/PrefSlider.qml
    qml/PrefSpin.qml
    qml/PrefSwitch.qml
    qml/PrefsDialog.qml
    qml/PrefsMode.qml
    qml/PrefsShortcuts.qml
    qml/PrefsSubtitles.qml
    qml/PrefsTorrents.qml
    qml/PrefsTranslators.qml
    qml/ProfileAvatar.qml
    qml/ProfileButton.qml
    qml/ProfilePopover.qml
    qml/ProgressRow.qml
    qml/PropertyList.qml
    qml/PullToRefresh.qml
    qml/QuickSearch.qml
    qml/RailNav.qml
    qml/ReaderPage.qml
    qml/ReaderProgress.qml
    qml/ReaderQuizCard.qml
    qml/ReaderSheet.qml
    qml/ReaderSidebar.qml
    qml/ReaderThemes.js
    qml/ResearchPage.qml
    qml/ResearchSheet.qml
    qml/ResearchSidebar.qml
    qml/ResearchTabRow.qml
    qml/ReviewCard.qml
    qml/ReviewsSection.qml
    qml/RichCard.qml
    qml/RoundedPicture.qml
    qml/SearchEntry.qml
    qml/SearchPage.qml
    qml/SectionCard.qml
    qml/SectionHead.qml
    qml/Segmented.qml
    qml/SeriesPage.qml
    qml/SheetLink.qml
    qml/SheetSegments.qml
    qml/ShelfCard.qml
    qml/SidebarNav.qml
    qml/SimilarRow.qml
    qml/SourceIcon.qml
    qml/StatusPage.qml
    qml/StreamRow.qml
    qml/StudyAdd.qml
    qml/StudyBrowse.qml
    qml/StudyBuddy.qml
    qml/StudyHeader.qml
    qml/StudyPage.qml
    qml/StudyQuiz.qml
    qml/StudyReview.qml
    qml/StudySettings.qml
    qml/StudySidebar.qml
    qml/StudyStats.qml
    qml/StudySurface.qml
    qml/StudyTime.qml
    qml/StudyTimeBeacon.qml
    qml/SubsMenu.qml
    qml/SwatchGrid.qml
    qml/TaskPanel.qml
    qml/TextLookSheet.qml
    qml/TextPage.qml
    qml/ThemeCard.qml
    qml/TitlePrompt.qml
    qml/ToolCard.qml
    qml/TrimTimeline.qml
    qml/WinButton.qml
    qml/WindowsColorPicker.qml
    qml/XRayPanel.qml)
# Study > Conversation and the local AI (Ollama): desktop only. The pages that show them load these files by URL
# (Loader { source: "KaiwaPage.qml" }), so their absence on Android and iOS breaks nothing.
if(NOT REELHOUSE_MOBILE)
    qt_add_resources(reelhouse-qt "desktop_ai" PREFIX "/" FILES
        qml/KaiwaPage.qml
        qml/KaiwaLiveBar.qml
        qml/KaiwaVoiceCard.qml
        qml/MicRecorder.qml
        qml/MicTest.qml
        qml/OllamaPanel.qml
        qml/VoicePanel.qml
        qml/VoiceHost.qml
        qml/LocalTts.qml
        qml/HeardStrip.qml)
endif()

# Open-source fonts of the themes and the readers (SIL OFL 1.1, licences beside them), loaded at startup by ThemeManager.
qt_add_resources(reelhouse-qt "fonts" PREFIX "/" FILES
    fonts/Cantarell-Bold.ttf
    fonts/Cantarell-Regular.ttf
    fonts/Inter-Bold.ttf
    fonts/Inter-Medium.ttf
    fonts/Inter-Regular.ttf
    fonts/Inter-SemiBold.ttf
    fonts/JetBrainsMono-Bold.ttf
    fonts/JetBrainsMono-Regular.ttf
    fonts/Literata-Bold.ttf
    fonts/Literata-BoldItalic.ttf
    fonts/Literata-Italic.ttf
    fonts/Literata-Regular.ttf
    fonts/Lora-Bold.ttf
    fonts/Lora-BoldItalic.ttf
    fonts/Lora-Italic.ttf
    fonts/Lora-Medium.ttf
    fonts/Lora-Regular.ttf
    fonts/Lora-SemiBold.ttf
    fonts/OFL-Cantarell.txt
    fonts/OFL-Inter.txt
    fonts/OFL-JetBrainsMono.txt
    fonts/OFL-Literata.txt
    fonts/OFL-Lora.txt
    fonts/OFL-PTSerif.txt
    fonts/OFL-Roboto.txt
    fonts/OFL-RobotoMono.txt
    fonts/PTSerif-Bold.ttf
    fonts/PTSerif-Italic.ttf
    fonts/PTSerif-Regular.ttf
    fonts/Roboto-Bold.ttf
    fonts/Roboto-Medium.ttf
    fonts/Roboto-Regular.ttf
    fonts/RobotoMono-Bold.ttf
    fonts/RobotoMono-Regular.ttf)
# The app icon drawn in the style of each theme (sources: icon/themes/*.svg); the window icon follows the theme.
qt_add_resources(reelhouse-qt "appicons" PREFIX "/" FILES
    icon/themes/reelhouse-apple-256.png
    icon/themes/reelhouse-apple-512.png
    icon/themes/reelhouse-gnome-256.png
    icon/themes/reelhouse-gnome-512.png
    icon/themes/reelhouse-windows-256.png
    icon/themes/reelhouse-windows-512.png
    icon/themes/reelhouse-android-256.png
    icon/themes/reelhouse-android-512.png)
# The controls style: Basic plus a themed ToolTip (found through the "qrc:/" import path).
qt_add_resources(reelhouse-qt "style" PREFIX "/" BASE style FILES
    style/ReelStyle/qmldir
    style/ReelStyle/ToolTip.qml)
# The macaw mascot (Mascot.qml): SVG moods, with WebP copies for a Qt without the SVG plugin.
qt_add_resources(reelhouse-qt "mascot" PREFIX "/" FILES
    mascot/arara-cheer-b.svg
    mascot/arara-cheer-blink.svg
    mascot/arara-cheer.svg
    mascot/arara-fly-b.svg
    mascot/arara-fly-blink.svg
    mascot/arara-fly.svg
    mascot/arara-happy-blink.svg
    mascot/arara-happy.svg
    mascot/arara-idle-blink.svg
    mascot/arara-idle.svg
    mascot/arara-peek-blink.svg
    mascot/arara-peek.svg
    mascot/arara-pull-b.svg
    mascot/arara-pull-blink.svg
    mascot/arara-pull.svg
    mascot/arara-push-b.svg
    mascot/arara-push-blink.svg
    mascot/arara-push.svg
    mascot/arara-sad-blink.svg
    mascot/arara-sad.svg
    mascot/arara-shh-blink.svg
    mascot/arara-shh.svg
    mascot/arara-sleep-blink.svg
    mascot/arara-sleep.svg
    mascot/arara-surprised-blink.svg
    mascot/arara-surprised.svg
    mascot/arara-think-blink.svg
    mascot/arara-think.svg
    mascot/arara-tired-blink.svg
    mascot/arara-tired.svg
    mascot/arara-wave-b.svg
    mascot/arara-wave-blink.svg
    mascot/arara-wave.svg
    mascot/webp/arara-cheer-b.webp
    mascot/webp/arara-cheer-blink.webp
    mascot/webp/arara-cheer.webp
    mascot/webp/arara-fly-b.webp
    mascot/webp/arara-fly-blink.webp
    mascot/webp/arara-fly.webp
    mascot/webp/arara-happy-blink.webp
    mascot/webp/arara-happy.webp
    mascot/webp/arara-idle-blink.webp
    mascot/webp/arara-idle.webp
    mascot/webp/arara-peek-blink.webp
    mascot/webp/arara-peek.webp
    mascot/webp/arara-pull-b.webp
    mascot/webp/arara-pull-blink.webp
    mascot/webp/arara-pull.webp
    mascot/webp/arara-push-b.webp
    mascot/webp/arara-push-blink.webp
    mascot/webp/arara-push.webp
    mascot/webp/arara-sad-blink.webp
    mascot/webp/arara-sad.webp
    mascot/webp/arara-shh-blink.webp
    mascot/webp/arara-shh.webp
    mascot/webp/arara-sleep-blink.webp
    mascot/webp/arara-sleep.webp
    mascot/webp/arara-surprised-blink.webp
    mascot/webp/arara-surprised.webp
    mascot/webp/arara-think-blink.webp
    mascot/webp/arara-think.webp
    mascot/webp/arara-tired-blink.webp
    mascot/webp/arara-tired.webp
    mascot/webp/arara-wave-b.webp
    mascot/webp/arara-wave-blink.webp
    mascot/webp/arara-wave.webp
)
target_include_directories(reelhouse-qt PRIVATE src)
target_link_libraries(reelhouse-qt PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Gui Qt6::OpenGL Qt6::Network Qt6::Concurrent
    "${RUST_LIB}")
if(NOT REELHOUSE_MOBILE AND TARGET Qt6::Multimedia)
    target_link_libraries(reelhouse-qt PRIVATE Qt6::Multimedia)
    # the microphone with the learner's turns (QML type VoiceInput)
    target_sources(reelhouse-qt PRIVATE src/voiceinput.cpp src/voiceinput.h)
    target_compile_definitions(reelhouse-qt PRIVATE REELHOUSE_VOICE_INPUT)
    # the microphone and speaker of Conversation by voice (Gemini Live, src/live.rs): desktop only
    target_sources(reelhouse-qt PRIVATE src/liveaudio.cpp src/liveaudio.h)
    target_compile_definitions(reelhouse-qt PRIVATE REELHOUSE_LIVE_AUDIO=1)
endif()
# Linked so that windeployqt6 copies Qt6TextToSpeech.dll and its plugins (the QML of LocalTts.qml imports it).
# REELHOUSE_TTS makes main.cpp call into it: a library only linked, never called, is dropped from the .exe's imports
# by MinGW, and windeployqt6 then skips it (the Windows build had no system voices for that reason).
if(NOT REELHOUSE_MOBILE AND TARGET Qt6::TextToSpeech)
    target_link_libraries(reelhouse-qt PRIVATE Qt6::TextToSpeech)
    target_compile_definitions(reelhouse-qt PRIVATE REELHOUSE_TTS)
    message(STATUS "Qt TextToSpeech found: the system voices (SAPI on Windows) are built in")
elseif(NOT REELHOUSE_MOBILE)
    message(WARNING "Qt TextToSpeech not found (MSYS2: mingw-w64-x86_64-qt6-speech): no system voices in this build")
endif()
if(WIN32)
    # The program icon of the .exe (Explorer, taskbar, Alt+Tab).
    target_sources(reelhouse-qt PRIVATE icon/reelhouse.rc)
    target_link_libraries(reelhouse-qt PRIVATE ws2_32 userenv bcrypt ntdll advapi32 kernel32 user32 shell32 ole32 oleaut32 crypt32 secur32 ncrypt gdi32 opengl32 synchronization)
elseif(ANDROID)
    # what the Rust standard library and its C parts (QuickJS, libwebp, ring) ask of bionic
    target_link_libraries(reelhouse-qt PRIVATE log android dl m)
elseif(IOS)
    # the certificate store (rustls-native-certs), the random source (ring) and UIKit for platform_ios.mm
    target_link_libraries(reelhouse-qt PRIVATE "-framework UIKit" "-framework Security" "-framework CoreFoundation" "-framework Foundation")
elseif(APPLE)
    # macOS: what the core's static library asks of the system (game controllers through gilrs: IOKit and
    # CoreFoundation; the standard library: libiconv)
    target_link_libraries(reelhouse-qt PRIVATE "-framework IOKit" "-framework CoreFoundation" "-framework Security" iconv)
else()
    target_link_libraries(reelhouse-qt PRIVATE pthread dl m)
endif()
set_target_properties(reelhouse-qt PROPERTIES WIN32_EXECUTABLE ON)

# ---- Android: the APK ----
# Phones have no folder next to the program: the course packs (learn/*.json) and the kana sounds ride inside the
# app as resources, and main.cpp copies them to the data folder on start, where the core looks (HP: "inclua no app").
if(REELHOUSE_MOBILE)
    # absolute paths: FILES are read from the source folder, BASE only sets the name inside the resource
    file(GLOB _reelhouse_learn "${CMAKE_SOURCE_DIR}/../assets/learn/*.json" "${CMAKE_SOURCE_DIR}/../assets/kana/*.ogg")
    if(_reelhouse_learn)
        qt_add_resources(reelhouse-qt "bundled" PREFIX "/bundled" BASE "${CMAKE_SOURCE_DIR}/../assets"
            FILES ${_reelhouse_learn})
    endif()
endif()

# Package sources (manifest, icon) live in android/. The core's `mobile` feature keeps the helper programs out; libmpv
# comes as a prebuilt (third_party/mpv-android/<abi>/*.so, see docs/mobile-build.md) and goes into the APK with
# QT_ANDROID_EXTRA_LIBS, where the core loads it by name ("libmpv.so"). Without it the app runs, but no video plays.
if(ANDROID)
    set_target_properties(reelhouse-qt PROPERTIES
        QT_ANDROID_PACKAGE_SOURCE_DIR "${CMAKE_SOURCE_DIR}/android"
        QT_ANDROID_MIN_SDK_VERSION ${REELHOUSE_ANDROID_MIN_SDK}
        QT_ANDROID_TARGET_SDK_VERSION ${REELHOUSE_ANDROID_TARGET_SDK}
        QT_ANDROID_VERSION_NAME "0.3.0"
        QT_ANDROID_VERSION_CODE 3)
    file(GLOB _reelhouse_mpv_android "${CMAKE_SOURCE_DIR}/third_party/mpv-android/${CMAKE_ANDROID_ARCH_ABI}/*.so")
    if(_reelhouse_mpv_android)
        set_property(TARGET reelhouse-qt APPEND PROPERTY QT_ANDROID_EXTRA_LIBS ${_reelhouse_mpv_android})
        message(STATUS "libmpv for Android: ${_reelhouse_mpv_android}")
    else()
        message(WARNING "no libmpv for Android in third_party/mpv-android/${CMAKE_ANDROID_ARCH_ABI}/: the APK will play no video (docs/mobile-build.md)")
    endif()
    # HTTPS for Qt (posters, covers, backgrounds through QNetworkAccessManager): Qt for Android ships no OpenSSL, and
    # without it every https picture fails. KDAB's prebuilt OpenSSL (third_party/android_openssl, fetched by
    # build-android.sh) goes into the APK next to Qt.
    if(EXISTS "${CMAKE_SOURCE_DIR}/third_party/android_openssl/android_openssl.cmake")
        include("${CMAKE_SOURCE_DIR}/third_party/android_openssl/android_openssl.cmake")
        add_android_openssl_libraries(reelhouse-qt)
        message(STATUS "OpenSSL for Android: third_party/android_openssl")
    else()
        message(WARNING "no third_party/android_openssl: pictures over https will not load in the APK")
    endif()
endif()

# ---- iOS: the app bundle ----
# Info.plist, the launch screen and the icon live in ios/. The build is unsigned unless REELHOUSE_IOS_TEAM (the Apple
# Developer team id) is given; libmpv comes as MPVKit's xcframeworks (third_party/mpvkit/*.xcframework, see
# docs/mobile-build.md), embedded in the bundle and loaded by the core at run time ("@rpath/Libmpv.framework/Libmpv").
if(IOS)
    set(REELHOUSE_BUNDLE_ID "org.reelhouse.app" CACHE STRING "Bundle identifier of the iOS app")
    set(REELHOUSE_IOS_TEAM "" CACHE STRING "Apple Developer team id for signing (empty = unsigned build)")
    target_sources(reelhouse-qt PRIVATE src/platform_ios.mm ios/Assets.xcassets)
    set_source_files_properties(ios/Assets.xcassets PROPERTIES MACOSX_PACKAGE_LOCATION Resources)
    set_target_properties(reelhouse-qt PROPERTIES
        MACOSX_BUNDLE ON
        MACOSX_BUNDLE_INFO_PLIST "${CMAKE_SOURCE_DIR}/ios/Info.plist"
        MACOSX_BUNDLE_GUI_IDENTIFIER "${REELHOUSE_BUNDLE_ID}"
        MACOSX_BUNDLE_BUNDLE_NAME "ReelHouse"
        MACOSX_BUNDLE_BUNDLE_VERSION "3"
        MACOSX_BUNDLE_SHORT_VERSION_STRING "0.3.0"
        QT_IOS_LAUNCH_SCREEN "${CMAKE_SOURCE_DIR}/ios/LaunchScreen.storyboard"
        XCODE_ATTRIBUTE_PRODUCT_BUNDLE_IDENTIFIER "${REELHOUSE_BUNDLE_ID}"
        XCODE_ATTRIBUTE_TARGETED_DEVICE_FAMILY "1,2"
        XCODE_ATTRIBUTE_IPHONEOS_DEPLOYMENT_TARGET "14.0"
        XCODE_ATTRIBUTE_ASSETCATALOG_COMPILER_APPICON_NAME "AppIcon"
        XCODE_ATTRIBUTE_LD_RUNPATH_SEARCH_PATHS "@executable_path/Frameworks")
    if(REELHOUSE_IOS_TEAM)
        set_target_properties(reelhouse-qt PROPERTIES
            XCODE_ATTRIBUTE_DEVELOPMENT_TEAM "${REELHOUSE_IOS_TEAM}"
            XCODE_ATTRIBUTE_CODE_SIGN_STYLE "Automatic")
    else()
        set_target_properties(reelhouse-qt PROPERTIES
            XCODE_ATTRIBUTE_CODE_SIGNING_ALLOWED "NO"
            XCODE_ATTRIBUTE_CODE_SIGNING_REQUIRED "NO"
            XCODE_ATTRIBUTE_CODE_SIGN_IDENTITY "")
    endif()
    file(GLOB _reelhouse_mpvkit "${CMAKE_SOURCE_DIR}/third_party/mpvkit/*.xcframework")
    if(_reelhouse_mpvkit)
        if(CMAKE_VERSION VERSION_LESS 3.28)
            message(WARNING "embedding .xcframework bundles needs CMake 3.28 or newer: no libmpv in the app")
        else()
            set_target_properties(reelhouse-qt PROPERTIES
                XCODE_EMBED_FRAMEWORKS "${_reelhouse_mpvkit}"
                XCODE_EMBED_FRAMEWORKS_CODE_SIGN_ON_COPY ON
                XCODE_EMBED_FRAMEWORKS_REMOVE_HEADERS_ON_COPY ON)
            message(STATUS "MPVKit for iOS: ${_reelhouse_mpvkit}")
        endif()
    else()
        message(WARNING "no MPVKit in third_party/mpvkit/: the iOS app will play no video (docs/mobile-build.md)")
    endif()
endif()

# ---- macOS: the app bundle ----
# reelhouse-qt.app. .github/workflows/macos.yml then adds Qt (macdeployqt), libmpv from Homebrew in Contents/Frameworks
# (where the core looks for it, src/runtime.rs), the icon (reelhouse.icns) and makes the .dmg.
if(APPLE AND NOT IOS)
    set_target_properties(reelhouse-qt PROPERTIES
        MACOSX_BUNDLE ON
        MACOSX_BUNDLE_GUI_IDENTIFIER "org.reelhouse.app"
        MACOSX_BUNDLE_BUNDLE_NAME "ReelHouse"
        MACOSX_BUNDLE_ICON_FILE "reelhouse.icns"
        MACOSX_BUNDLE_BUNDLE_VERSION "3"
        MACOSX_BUNDLE_SHORT_VERSION_STRING "0.3.0")
endif()

# ---- Research mode: Chromium (CEF, the Chromium Embedded Framework, BSD) draws the pages off screen into WebPane ----
# qt/third_party/cef is CEF's "minimal" distribution (include/, Release/, Resources/) that msys-build-qt.sh
# downloads. Only its C headers are used (no MSVC-only libcef_dll_wrapper) and libcef is loaded at run time, so the
# app builds and starts without it: Research mode then opens pages in the system browser.
set(CEF_ROOT "${CMAKE_SOURCE_DIR}/third_party/cef")
if(EXISTS "${CEF_ROOT}/include/capi/cef_app_capi.h" AND NOT REELHOUSE_MOBILE)
    set(_reelhouse_cef_default ON)
else()
    set(_reelhouse_cef_default OFF)
endif()
option(REELHOUSE_CEF "Research mode pages drawn by Chromium (CEF) from third_party/cef" ${_reelhouse_cef_default})
if(REELHOUSE_MOBILE AND REELHOUSE_CEF)
    # Chromium has no embeddable build for phones: Research mode opens pages in the system browser there.
    message(STATUS "REELHOUSE_CEF ignored on Android / iOS")
    set(REELHOUSE_CEF OFF)
endif()
# the version of CEF's C API the code is written against (CEF 154); libcef checks it at start
set(REELHOUSE_CEF_API 15400)
target_sources(reelhouse-qt PRIVATE src/cef_host.cpp src/cef_host.h)
# Android: the pages are drawn by the Android System WebView (Qt WebView) instead (src/webview_host.cpp)
if(ANDROID)
    find_package(Qt6 6.6 QUIET COMPONENTS WebView)
    if(TARGET Qt6::WebView)
        target_sources(reelhouse-qt PRIVATE src/webview_host.cpp)
        target_link_libraries(reelhouse-qt PRIVATE Qt6::WebView)
        qt_add_resources(reelhouse-qt "webview" PREFIX "/" FILES qml-android/WebViewTab.qml qml-android/SiteCheck.qml)
        target_compile_definitions(reelhouse-qt PRIVATE REELHOUSE_WEBVIEW=1)
        message(STATUS "Research mode: Android System WebView (Qt WebView)")
    else()
        message(WARNING "Qt WebView not found: Research mode opens pages in the system browser on Android")
    endif()
endif()
if(REELHOUSE_CEF)
    enable_language(C)
    target_include_directories(reelhouse-qt PRIVATE "${CEF_ROOT}")
    target_compile_definitions(reelhouse-qt PRIVATE REELHOUSE_CEF=1 CEF_API_VERSION=${REELHOUSE_CEF_API})
    # the small program Chromium starts its page, GPU and network processes with (it goes next to reelhouse-qt)
    add_executable(reelhouse-cef-helper cefhelper/main.c)
    target_include_directories(reelhouse-cef-helper PRIVATE "${CEF_ROOT}")
    target_compile_definitions(reelhouse-cef-helper PRIVATE CEF_API_VERSION=${REELHOUSE_CEF_API})
    # its frames sit under the zygote's fork on Linux: no stack canary checks there (see main.c)
    target_compile_options(reelhouse-cef-helper PRIVATE -fno-stack-protector)
    set_target_properties(reelhouse-cef-helper PROPERTIES RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}")
    if(WIN32)
        target_sources(reelhouse-cef-helper PRIVATE cefhelper/helper.rc)
        set_target_properties(reelhouse-cef-helper PROPERTIES WIN32_EXECUTABLE ON)
    else()
        target_link_libraries(reelhouse-cef-helper PRIVATE dl)
    endif()
    add_dependencies(reelhouse-qt reelhouse-cef-helper)
endif()
