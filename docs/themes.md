# Reelhouse themes (Qt version)

A theme is **one JSON file** in `qt/themes/` (listed in `qt/CMakeLists.txt`).
The person picks it in Preferences > General > Appearance > **Style**; the choice is `general.theme` and takes effect at once.
Pages never hard-code a colour, radius, font or duration: they read `Theme.c.*`, `Theme.shape.*`, `Theme.type.*`, `Theme.spacing.*`,
`Theme.motion.*`, `Theme.material.*` and `Theme.layout.*` (`qt/src/thememanager.*`). Light/dark and the accent colour (Preferences) are
applied on top of any theme.

| Theme | File | State |
|---|---|---|
| GNOME (libadwaita): header bar with view switcher, solid popovers | `gnome.json` | done: it is the look of the app before themes, saved as tokens |
| Apple (HIG): floating glass sidebar, large titles, capsule controls | `apple.json` | done (QT33) |
| Windows (Fluent): NavigationView left rail, Mica/Acrylic | `windows.json` | specified below, not built |
| Android (Material 3): top app bar, bottom bar / rail, tonal surfaces | `android.json` | specified below, not built |

## Token groups (every theme file has all of them)

- `colors.dark` / `colors.light`: the same ~60 names in both sets (window, view, headerbar, card, popover, text, textDim, accent, accentText,
  accentFg, success, warning, error, rating, divider, border, hover, selected, button, buttonHover, chip, fill/fill2/fill3 (Apple tertiary fills),
  sidebarSelected, glassBorder, glassHighlight, …). The manager adds `accent20/25/28/50` and `window0..98` from the accent and window colours.
  It also checks `accentFg`, `greenFg`, `errorFg` and (new) `warningFg` against their fills (white or black if under 3:1), and adds
  `successText`, `warningText`, `errorText`: the status colour as words on the window, at 4.5:1. Use the `*Text` ones for
  words and icons, the plain ones for fills. Colours are `"#rrggbb"` text: use `Qt.alpha(Theme.c.x, a)`, never `Qt.rgba(Theme.c.x.r, …)`
  (that reads NaN and paints black). 8-digit colours are `#AARRGGBB`.
- `shape`: small, medium, large, poster, card, pill, border, focusRing, **popover**, **panel** (floating panels), **sheet** (dialogs), control.
- `elevation`: shadow levels (used by Glass for floating panels).
- `material`: `kind` = `solid` | `glass`. Glass themes also give `blurMax`, `saturation`, `clearDim` and four levels
  (`ultraThin`, `thin`, `regular`, `thick`), each with `blur` and a tint per scheme. Thicker = more opaque (HIG: contrast for text).
- `typography`: `family`/`mono` plus optional `familyStack`/`monoStack` (the manager picks the first installed font), role sizes and weights
  (`largeTitle`, `title`, `heading`, `body`, `caption`, `rowTitle`) and `buttonWeight`.
- `spacing`: scale (xs…xl), `page`, poster/continue card sizes, `headerH`, `sidebarW`, `sidebarInset`, `largeTitleH`.
- `motion`: fast / normal / slow (ms) and the easing name.
- `layout`: the structure (below).
- `icons`: name of the icon set (only the Adwaita-style set exists today; `sf-like` reuses it).

## `layout` choices (what the components switch on)

| key | values | read by |
|---|---|---|
| `nav` | `headerbar-tabs` (GNOME) · `sidebar` (Apple, Windows rail later) · `bottom-bar` (Android later) | `Main.qml` (`sideNav`), `AppHeader.qml`, `SidebarNav.qml` |
| `toolbar` | `headerbar` · `transparent` | `AppHeader.qml` |
| `largeTitles` | bool: a large title over the pages of the sidebar | `Main.qml` |
| `windowControls` | `native-right` · `traffic-lights-left` | `AppHeader.qml`, `SidebarNav.qml` |
| `dialog` | `centered` · `sheet` | dialogs use `Glass` with `shape.sheet` |
| `buttons` | `rounded` · `capsule` | `FlatButton.qml` |
| `toggle` | `adw` · `ios` | `PrefSwitch.qml` |
| `posterStyle` | `rounded-card` (accent ring on hover) · `soft` (lifts a little) | `PosterCard.qml` |
| `listRow` | `boxed` · `inset-grouped` | `PrefGroup.qml` |
| `aurora` | bool: ambient colours behind the glass | `Main.qml` |

## Engine pieces added with the Apple theme

- `Glass.qml`: the material. Solid panel for GNOME; for glass themes it reads the region of `Theme.backdrop` (the `stage` item of `Main.qml`
  that holds the window colour, the ambient colours and the pages) with a `ShaderEffectSource`, blurs it (`MultiEffect`), tints it by level
  and draws the hair line and top highlight. Used by `Popover`, `AppMenu`, every dialog and the sidebar.
- `SidebarNav.qml`: the Apple navigation (traffic lights, Video/Images/Text segmented control, search, places with accent icons, catalog,
  bell, reload, preferences, menu). The header turns into a toolbar; the pages start after the sidebar (`win.sideW`).
- `ThemeManager`: `Theme.list`, `Theme.glass`, `Theme.sidebarNav`, `Theme.backdrop`, font stacks.

## Apple (done): what comes from the HIG pages

Materials (Liquid Glass for navigation and controls, standard materials ultra-thin/thin/regular/thick for structure, legibility through
thicker material, clear variant needs 35 % dimming), Color (system colour values for light/dark, accent follows the user, symbols in the
sidebar use the accent), Typography (SF Pro styles; the stack falls back to Inter/Segoe UI Variable where SF is not installed),
Sidebars (floating over content, no more than two levels, accent icons), Layout (alignment, consistent spacing).
Not covered by the pages and chosen by us: sidebar width 236, radii (panel 22, popover 16, sheet 24), the iOS switch size 51x31.
Icons are still the shared set (no SF Symbols on Windows).

`themes/ios.json` (look `apple`) is the same family for a phone: iOS system colours (label, fills, grouped backgrounds, separators, systemGray1-6),
SF Dynamic Type sizes, 44 pt targets, 49 pt tab bar, sheets with a grabber, no FAB.

## Windows (done: Fluent, `themes/windows.json`)
Read: design principles, geometry, typography, materials, NavigationView (learn.microsoft.com). Applied:
- Geometry: 4 px controls and 8 px overlays/cards (`shape.control/large/popover/sheet`), 1 px strokes, 2 px focus.
- Layout: `nav: rail` + `paneStyle: docked`: a title bar (40 px: app name, back button, 46x32 caption buttons, red close on hover,
  `windowControls: native-right`, `captionButtons: fluent`) over a docked left pane (272 px, flush, full height, same colour as the window =
  Mica) and the content on a `Theme.c.layer` sheet with a rounded top-left corner (`contentLayer`). Selected row: fill + 3x16 accent pill on
  the left (`navIndicator: pill-left`), icons in the text colour (`navIconColor: text`), 40 px rows, Settings/bell/refresh/menu at the bottom.
- Segmented mode switch and search are rectangular (radius 4); the search gets an accent bottom line on focus.
- Typography: Segoe UI Variable stack, Regular/Semibold, ramp 12/14/20/28/40, Cascadia for mono. Large titles 28 px semibold.
- Material: acrylic (`glass` levels, light blur) only for flyouts/menus; the window itself is solid.
- Icons: Lucide at stroke 1.5 (`icons: lucide`), the closest free match to Fluent line icons.
- New tokens: `layout.paneStyle` (floating|docked), `navIndicator` (fill|pill-left), `navIconColor`, `segmented`, `captionButtons`
  (round|fluent|m3), `titleBarTitle`, `contentLayer`; `spacing.navRowH/navPad/paneCompactW`. Compact pane: under 1000 px wide the pane becomes a 48 px icon strip (`RailNav` in its flat flavour, tooltips for the names); the menu button of the title bar flips open/compact (`Main.paneMode`).

## Lucide icons
`qml/LucideIcons.js` maps the app's icon names to Lucide paths (ISC, `themes/LICENSE-lucide.txt`). A theme opts in with `icons: "lucide"`
and `iconStroke`; `Icon.qml` strokes the path instead of filling the old glyph. Subpaths written with a relative `m` must be absolute at
the start of each element (and keep `l` for the implicit line-to) when elements are joined into one path.

## Android (done: Material 3, `themes/android.json`)
The Android page is only an index; the specifics (navigation rail, top app bar, shapes, switch, dialogs, type) follow Material 3 (m3.material.io).
- Navigation: `nav: rail`, `paneStyle: rail` (`RailNav.qml`): 88 px rail, every place an icon with its label under it, a 56x32 pill behind the
  icon when selected (`navIndicator: pill-icon`); the mode (one button that opens Video/Images/Text) and Search at the top, bell/reload/settings/menu
  at the bottom. The rail runs the full height; the 64 px top bar sits to its right with a large title below (`toolbar: appbar`).
- Surfaces: `material.kind: tonal` (no blur): window/pane/layer share the surface colour, cards and menus are surface containers.
  Colours are the baseline M3 scheme (primary #6750A4 light / #D0BCFF dark).
- Shapes 8/12/16, sheets and dialogs 28; buttons full round (`buttons: capsule`); switch 52x32 (`toggle: m3`, thumb 16 off / 24 on).
- Type: Google Sans / Roboto; caption buttons round 40 px (`captionButtons: m3`) because a desktop window still needs them.
- Under 600 px the rail becomes `BottomNav.qml` (Material navigation bar: places, Search, and "More" with mode, catalog, reload, preferences, about, quit); the window minimum width drops to 420 for this theme. State layers beyond hover are not built.

## Lucide icons
`qml/LucideIcons.js` maps the app's icon names to Lucide paths (ISC, `themes/LICENSE-lucide.txt`). A theme opts in with `icons: "lucide"`
and `iconStroke`; `Icon.qml` strokes the path instead of filling the old glyph. Subpaths written with a relative `m` must be absolute at
the start of each element (and keep `l` for the implicit line-to) when elements are joined into one path.

## Mobile (done: Material 3 on Android, iOS HIG; `Theme.mobile`)

**Theme keys added (every theme loads with defaults, nothing renamed).**
- `look` (string): the family of the theme for the QML checks (`Theme.look === "apple"`); `ios.json` says `apple`, the others their id.
- `mobile` group (read as `Theme.mob`, defaults per family in `ThemeManager::pickMobile`): `target` (48 / 44 on iOS), `rowH` (56 / 44), `row2H`
  (72 / 60), `barH` (80 navigation bar / 49 tab bar), `appBarH` (64 / 44; 56 GNOME, 48 Windows), `fabSize` 56, `fabRadius` (16 / 28), `fab`
  (bool: Material has FABs, iOS does not), `nav` (`bar` | `tab-bar`), `dialog` (`fullscreen` | `sheet`), `grabber` (bool, sheets with a handle),
  `navLabel` (12 / 11), `minText` 14, `seekStep` 10, `pageGutter` 16, `maxTabs` 5.
- Material 3 colour roles in `colors.light/dark` (fallbacks in `pickColors`): `primaryContainer`, `onPrimary`, `onPrimaryContainer`, `secondary`,
  `secondaryContainer`, `onSecondaryContainer`, `tertiary*`, `surfaceContainerLowest/Low/Container/High/Highest`, `surfaceVariant`, `onSurface`,
  `onSurfaceVariant`, `outline`, `outlineVariant`, `inverseSurface`, `scrim`, `stateHover/Focus/Pressed/Dragged`, `fab`, `onFab`, `tabBar`, `grabber`.
- iOS roles: `label2/3/4`, `fill1..4`, `groupedBg`, `groupedCard`, `separator`, `tabBar`, `grabber`, `systemGray1..6`.
- `shape.fab`, `shape.chip`, `shape.extraSmall..extraLarge`; `elevation.tonal`; `type.mobile` sizes and `type.textStyles` (M3 roles / SF Dynamic
  Type); `spacing.touchTarget`; `layout.stateLayers`. `android.json` and `ios.json` carry a `schema` object that documents their keys.
- `Theme.mobile` (bool, written by Main.qml: Android/iOS, or a window narrower than 600 px at scale 1), `Theme.touch` (a touch screen is present),
  `Theme.safe` {top,bottom,left,right} (safe-area insets, 0 until a Platform singleton feeds them).

**Mobile rules (gated by `Theme.mobile`, the desktop is unchanged).**
- Navigation: `BottomNav.qml`, 3-5 destinations (Video: Discover, Library, Calendar, Search, More; Images/Text: Library, Explore, History, More;
  Study: Decks, Learn, Talk, Time, More). Material: 80 dp bar, 64x32 pill; iOS: 49 pt tab bar + safe bottom, hairline, accent tint. "More" is a
  bottom sheet with the mode, catalog, notifications, reload, preferences, about, quit. No sidebar / rail on a phone.
- Large titles on every home page (also GNOME/Windows on a phone); the header bar keeps search, the mode label, bell and menu; the window
  buttons disappear on Android/iOS; in readers and detail pages the bar keeps only what fits.
- Targets: `FlatButton`, `ModeButton`, `OsdButton` are at least `Theme.mob.target`; rows 56/72 (`PrefRow`); texts >= 14 px. Hover tooltips have a
  long-press equivalent (`onPressAndHold` -> `ToolTip.show`), poster cards zoom on press and show their name on long press.
- Popups: `Popover` (and `OsdMenu`, `SubsMenu`) become bottom sheets (full width, scrim, grabber when `Theme.mob.grabber`, swipe down closes);
  `PrefsDialog` is full screen; `AboutDialog` fits the width.
- FAB (`Fab.qml`, only when `Theme.mob.fab`): Study "+" (add cards), series page "Start / Continue reading".
- Pull to refresh (`PullToRefresh.qml`) on Discover (`load_home`) and Library (`fill_library`); the library is one card per line.
- Detail page: the info panel takes the width, the streams panel sits under it, title 28 px, actions wrap (Flow), back / cover buttons in one row.
- Readers: portrait defaults kept per device (`QtCore.Settings`, category `mobileReader`): manga = `webtoon` (vertical strip), novels = paged
  (swipe turns the page). The sidebar tools stay in the header bar.
- Player: tap toggles the controls, double tap on the left / right third seeks `seekStep` seconds (flash shown), middle double tap plays /
  pauses; 80 px play button; the bar keeps play, next, skip, subtitles, More and fullscreen, the rest (quality, audio, speed, X-Ray, upscale,
  video settings, editor, stats, mute) lives in a More bottom sheet.
- Performance: no blur (`Glass.on` is false on a phone; the material tint is painted instead); pages behind the current one are `Loader`s
  (`Lazy` in Main.qml: Calendar, Search, Genre, Detail, Info, Images, Text); `HScrollList` reuses items; `PosterRow` is indexed by count;
  `RoundedPicture` enables its layer only when the picture is ready and asks a 1.5x source size.

## HyperOS (Xiaomi phones, `themes/hyperos.json`)

`look: android`, so every component behaves as in the Android theme; only the tokens and three switches differ:
- colours: page `#F7F7F7` with white cards (night: black page, `#1C1C1C` cards), HyperOS blue `#3482FF` accent, no hair lines;
- shapes: cards and groups 20, sheets 32, FAB a 56 circle; type MiSans (the system font on HyperOS; Roboto elsewhere), bold large titles;
- `layout.toggle: "hyper"` (PrefSwitch: iOS shape, accent when on, no knob border), `listRow: "inset-grouped"`;
- `mobile.nav: "tab-bar"` (flat bar, selected place in the accent), `mobile.floatingSheet` + `sheetMargin` (Popover: the bottom sheet floats 12 px from the edges, every corner rounded).
- Chosen automatically: `ThemeManager::systemId` is `hyperos` when `ro.mi.os.version.name` is set or the maker is Xiaomi / Redmi / POCO, else `android` / `ios` / `gnome`. `general.theme = ""` ("Automatic" in Preferences > Style) follows it; `REELHOUSE_THEME=<id>` forces one for testing.

## Study mode
The Study page (`docs/study.md`) uses only theme tokens (card, divider, accent/error/success colours, shapes, button style), so it follows every theme.


## Uniform look with GTK (Arch wiki "Uniform look for Qt and GTK applications")
Reelhouse draws every widget itself (QML "Basic" style), so the wiki's style fixes (adwaita-qt, QT_STYLE_OVERRIDE, qadwaitadecorations) do not change it: the GNOME theme imitates libadwaita on its own.
What is shared with the desktop: on GNOME `QT_QPA_PLATFORMTHEME=xdgdesktopportal` is set when you did not set it (file dialogs, colour scheme), the light/dark setting is followed live, and with no accent chosen the GNOME theme takes the system accent colour. Set `QT_QPA_PLATFORMTHEME` yourself to override.

## Colour chooser

`ColorPickerAdapter.qml` replaces the generic system colour dialog. It keeps one colour state (hex, RGB, HSV, alpha) and loads the chooser of the active theme (`Theme.id`): `GnomeColorPicker` (Libadwaita palette, plane, RGB/HSV sliders), `WindowsColorPicker` (Fluent spectrum with 8 px corners, basic colours, R G B boxes, acrylic panel), `AndroidColorPicker` (Material 3: dynamic colours from the system accent, tonal palette, M3 sliders) and `AppleColorPicker` (round wheel, brightness and opacity sliders, system colours). Same API as `ColorDialog` (`selectedColor`, `open()`, `onAccepted`), plus `withAlpha`. Shared parts: `ColorMath.js`, `ColorSlider`, `ChannelRow`, `ColorPlane`, `SwatchGrid`. Test: `REELHOUSE_TEST_OPEN=picker`.
