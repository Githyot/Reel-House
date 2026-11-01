# ReelHouse no Android e no iOS: como compilar

Estado em 15/10/2026. O núcleo Rust e a interface Qt/QML são os mesmos do Windows; o que muda é o que o telefone
pode fazer (sem programas auxiliares) e como o pacote é montado. Este arquivo diz o que já existe, como compilar em
casa e o que ainda falta.

## O que já está no código

| Parte | Como ficou |
|---|---|
| Núcleo Rust (`src/`, `qtbridge/`) | Compila para `aarch64-linux-android` e `aarch64-apple-ios`. O `build.rs` liga o cfg `reelhouse_mobile` nesses alvos (ou com a feature `mobile`). |
| Programas auxiliares | rqbit (torrent), Suwayomi (Java), Real-ESRGAN (upscale), yt-dlp (aberturas) e Ollama (IA local) **não existem no celular**: as funções que os iniciam respondem com uma mensagem clara (`src/platform.rs`). Real-Debrid, addons, MangaDex, plugins de novel, EPUB, estudo e leitores funcionam. Para mangá com extensões, use um Suwayomi remoto (Preferências). |
| Controles (gilrs) | Só Windows e macOS; no Android/iOS o D-pad e os botões chegam como teclas do Qt. |
| libmpv | O núcleo procura `libmpv.so` (Android) ou `@rpath/Libmpv.framework/Libmpv` (iOS, MPVKit). Os binários **não estão no repositório** (ver abaixo). |
| Navegador (CEF) | Só desktop. No celular o modo Pesquisa abre páginas no navegador do sistema. |
| C++ (`qt/src/platform.*`) | Singleton `Platform` no QML: `isMobile`, `isAndroid`, `isIOS`, `isDesktop`, `os`, `touch`, `tv` (Android TV), `safeAreaInsets` `{top,left,right,bottom}`, `caps`, `localProcesses`, `browser`, `fileDialogs`, `windowFrame`, `keepScreenOn(bool)`, `folders()`. No celular os dados vão para a pasta privada do app (`REELHOUSE_DATA`) e os logs para `<dados>/debug/`. |
| CMake (`qt/CMakeLists.txt`) | Ramos `if(ANDROID)` (APK, manifesto em `qt/android/`, SDK mínimo 28, alvo 34, `QT_ANDROID_EXTRA_LIBS` para o libmpv) e `if(IOS)` (bundle, `qt/ios/Info.plist`, tela de abertura, ícone, sem assinatura por padrão, MPVKit embutido quando presente). O ramo do Windows não mudou. |
| Scripts e CI | `build-android.sh`; `.github/workflows/android.yml` (Ubuntu, APK) e `.github/workflows/ios.yml` (macOS, `.ipa` sem assinatura). Os dois rodam à mão, em tags `v*` e em PRs que mudam os workflows, e anexam o pacote à Release como o `windows.yml`. |

## Android em casa (Linux ou macOS)

Precisa de: Android SDK (`platforms;android-34`, `build-tools;34.0.0`, `platform-tools`), NDK r26 ou r27, JDK 17,
Qt 6.8 para Android (`android_arm64_v8a`) **e** o Qt de desktop da mesma versão (ferramentas do host), rustup, cmake,
ninja, libclang (`apt install libclang-dev`; o QuickJS gera os bindings na hora, feature `js-bindgen`).

```bash
export ANDROID_SDK_ROOT=~/Android/Sdk
export ANDROID_NDK_ROOT=$ANDROID_SDK_ROOT/ndk/26.3.11579264
export QT_ANDROID=~/Qt/6.8.3/android_arm64_v8a
export QT_HOST=~/Qt/6.8.3/gcc_64        # macOS: ~/Qt/6.8.3/macos
./build-android.sh                      # REELHOUSE_ABI=x86_64 para o emulador
```

Sai em `dist/ReelHouse-android-arm64-v8a.apk`, assinado com a chave de depuração (instala com "fontes desconhecidas").
`SKIP_RUST=1` reaproveita o núcleo já compilado. O log fica em `build-android.log`.

Só o núcleo Rust, sem Qt:

```bash
cargo install cargo-ndk && rustup target add aarch64-linux-android
cargo ndk -t arm64-v8a --platform 28 build --release --manifest-path qtbridge/Cargo.toml \
  --target-dir target-android --no-default-features --features "mobile,js-bindgen"
```

### libmpv para Android (falta)

O APK sobe sem o mpv: o app abre e tudo funciona menos o vídeo. Para ter vídeo, coloque o `libmpv.so` e as
bibliotecas de que ele depende (ffmpeg, libass, libplacebo…) em `qt/third_party/mpv-android/arm64-v8a/`. Fontes:

- o projeto **mpv-android** (`github.com/mpv-android/mpv-android`): o script `buildscripts/` gera `libmpv.so` com
  tudo embutido; os APKs publicados também trazem `lib/arm64-v8a/libmpv.so` (é GPL, como o app);
- ou um build próprio do mpv com o NDK (`--enable-libmpv-shared`).

O CMake avisa quando a pasta está vazia. Não guarde esses binários no repositório: baixe no build (uma etapa no
workflow, com versão e checksum fixos, como o CEF no `msys-build-qt.sh`).

### Android TV

O mesmo APK: o manifesto declara `LEANBACK_LAUNCHER` e `android.software.leanback` como opcional, e `Platform.tv`
fica verdadeiro na TV. Falta o layout com foco por controle remoto (tarefa da interface).

## iOS em casa (só macOS, com Xcode)

Precisa de: Xcode 15+, Qt 6.8 para iOS (`ios`) **e** o Qt para macOS da mesma versão, rustup com o alvo
`aarch64-apple-ios` (e `aarch64-apple-ios-sim` para o simulador), cmake 3.28+ (para embutir `.xcframework`).

```bash
rustup target add aarch64-apple-ios
export SDKROOT="$(xcrun --sdk iphoneos --show-sdk-path)"
export BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios="-isysroot $SDKROOT -miphoneos-version-min=14.0"
cargo build --release --target aarch64-apple-ios --manifest-path qtbridge/Cargo.toml \
  --target-dir target-ios --no-default-features --features "mobile,js-bindgen"

~/Qt/6.8.3/ios/bin/qt-cmake -S qt -B build-ios -G Xcode -DQT_HOST_PATH=~/Qt/6.8.3/macos \
  -DCMAKE_OSX_SYSROOT=iphoneos -DREELHOUSE_BUILD_RUST=OFF \
  -DREELHOUSE_RUST_LIB=$PWD/target-ios/aarch64-apple-ios/release/libreelhouse_qtbridge.a
# sem assinatura (não instala no aparelho; serve para ver se compila):
cmake --build build-ios --config Release -- -sdk iphoneos CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO
# com a conta Apple do HP (assinatura automática do Xcode):
~/Qt/6.8.3/ios/bin/qt-cmake ... -DREELHOUSE_IOS_TEAM=<team id>
```

Para o simulador: `-DREELHOUSE_IOS_SIMULATOR=ON -DCMAKE_OSX_SYSROOT=iphonesimulator` e o núcleo em
`aarch64-apple-ios-sim`. Abrir `build-ios/reelhouse_qt.xcodeproj` no Xcode também funciona.

### MPVKit para iOS (falta)

Coloque os `.xcframework` do **MPVKit** (`github.com/mpvkit/MPVKit`, releases: `Libmpv.xcframework` e as
dependências `Libavcodec`, `Libass`, `Libplacebo`…) em `qt/third_party/mpvkit/`. O CMake embute todos no bundle
(`XCODE_EMBED_FRAMEWORKS`) e o núcleo carrega `@rpath/Libmpv.framework/Libmpv` na hora. Sem eles, o app abre e não toca
vídeo. O MPVKit tem variante GPL e LGPL; a GPL é a completa.

### Assinatura e lojas

- **Nada é assinado** no CI: o `.ipa` publicado é só para conferir o build. Para instalar: assinar com a conta Apple do
  HP (Xcode com `REELHOUSE_IOS_TEAM`, ou AltStore/Sideloadly com um Apple ID gratuito, válido 7 dias; com conta de
  desenvolvedor paga, TestFlight).
- **App Store:** o app é GPL-3 e embute o libmpv (GPL): como está, não entra. Também não aceitam torrent nem código
  baixado para executar (plugins JavaScript de novel). Ver `plano-multiplataforma.md`, seção 6.
- **Google Play:** torrent e extensões de mangá violam a política; o caminho é APK no GitHub Releases (e F-Droid).
  Para o Play só uma versão "lite".
- O ícone do iOS (`qt/ios/Assets.xcassets/AppIcon.appiconset/reelhouse-ios-1024.png`) já é 1024×1024 sem
  transparência, como a loja exige. Todos os ícones (Android, iOS, Windows, Linux, macOS) saem de
  `python3 tools/app_icon.py` (precisa de rsvg-convert e ImageMagick).

## Como o código decide o que existe

- Rust: `cfg(reelhouse_mobile)` (ligado pelo `build.rs` nos alvos Android/iOS ou pela feature `mobile`),
  `reelhouse::platform::{MOBILE, local_processes(), caps()}`. A ponte expõe `rh_platform_caps()`.
- C++: `REELHOUSE_MOBILE` (definido em `qt/src/platform.h` quando `Q_OS_ANDROID` ou `Q_OS_IOS`); CEF nunca entra no
  celular (`REELHOUSE_CEF` forçado OFF).
- QML: `Platform.isMobile`, `Platform.safeAreaInsets`, `Platform.touch`… (a parte de interface está com outro agente).

## O que ainda falta

1. **libmpv/MPVKit** no pacote (etapa de download com checksum nos workflows). Sem isso, sem vídeo.
2. **Assinatura iOS** (conta Apple do HP) e decisão de licença para loja.
3. **Testar de verdade**: nada aqui rodou num telefone ou num emulador. Os workflows foram escritos a partir da
   documentação do Qt e das actions; a primeira execução vai apontar o que ajustar.
4. **Torrent como biblioteca** (`librqbit`) para o celular, em vez de processo; serviço em primeiro plano no Android.
5. **Permissões em tempo de execução** (microfone, arquivos) e seleção de arquivos por `content://` no Android.
6. **Dados móveis / espaço**: aviso de rede celular, limite de cache.
7. Layouts de toque e de TV (outro agente).
