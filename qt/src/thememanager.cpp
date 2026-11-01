#include "thememanager.h"
#include <QDir>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QDebug>
#include <QColor>
#include <QGuiApplication>
#include <QStyleHints>
#include <QPalette>
#include <QFontDatabase>
#include <QJsonArray>
#include <QFont>
#include <QQuickWindow>
#include <QWindow>
#include <QIcon>
#include <QInputDevice>
#include <QSysInfo>
#include <algorithm>
#include <cmath>
#ifdef Q_OS_ANDROID
#include <sys/system_properties.h>
#endif

// The fonts that ship inside the program (qt/fonts, SIL OFL): Inter (Apple/Windows fallback), Cantarell (GNOME),
// Roboto + Roboto Mono (Android), JetBrains Mono, and the reading fonts Literata, Lora and PT Serif. They are added
// once, before the QML engine loads, so every theme and reader can name them like installed fonts.
static void loadBundledFonts() {
    static bool done = false;
    if (done || !QGuiApplication::instance()) return;
    done = true;
    const auto files = QDir(":/fonts").entryList({"*.ttf", "*.otf"}, QDir::Files, QDir::Name);
    for (const QString &f : files)
        if (QFontDatabase::addApplicationFont(":/fonts/" + f) < 0) qWarning() << "font not loaded:" << f;
}

static bool s_ownPalette = false;

#ifdef Q_OS_ANDROID
// One Android system property ("" when it is not set).
static QString sysProp(const char *name) {
    char v[PROP_VALUE_MAX] = {0};
    return __system_property_get(name, v) > 0 ? QString::fromLatin1(v).trimmed() : QString();
}
#endif

// The theme that looks like this device's own system. Xiaomi phones (Xiaomi, Redmi, POCO) on HyperOS set
// ro.mi.os.version.name ("OS1.0", "OS2.0"...); the brand is checked too, so a build that leaves the property out
// still gets the HyperOS look.
QString ThemeManager::detectSystemTheme() {
    const QString forced = qEnvironmentVariable("REELHOUSE_THEME");
    if (!forced.isEmpty() && QFile::exists(":/themes/" + forced + ".json")) return forced;
#ifdef Q_OS_ANDROID
    const QString hyper = sysProp("ro.mi.os.version.name");
    const QString maker = (sysProp("ro.product.manufacturer") + " " + sysProp("ro.product.brand")).toLower();
    const bool xiaomi = maker.contains("xiaomi") || maker.contains("redmi") || maker.contains("poco");
    qInfo() << "system theme: maker" << maker << "HyperOS" << (hyper.isEmpty() ? QStringLiteral("-") : hyper);
    return !hyper.isEmpty() || xiaomi ? QStringLiteral("hyperos") : QStringLiteral("android");
#elif defined(Q_OS_IOS)
    return QStringLiteral("ios");
#else
    return QStringLiteral("gnome");
#endif
}

ThemeManager::ThemeManager(QObject *parent) : QObject(parent) {
    loadBundledFonts();
    if (QGuiApplication::instance()) {
        connect(QGuiApplication::styleHints(), &QStyleHints::colorSchemeChanged, this, [this] { followSystem(); });
        connect(qGuiApp, &QGuiApplication::paletteChanged, this, [this] { if (!s_ownPalette) followSystem(); });
    }
    // A phone or tablet build lays the pages out for a phone from the start; a touch screen on a desktop only adds
    // the tap equivalents of the hover affordances. Main.qml refines `mobile` with the window width.
    const QString os = QSysInfo::productType();
    m_mobile = os == "android" || os == "ios";
    m_touch = m_mobile;
    if (!m_touch) {
        const auto devs = QInputDevice::devices();
        for (const QInputDevice *d : devs) if (d && d->type() == QInputDevice::DeviceType::TouchScreen) { m_touch = true; break; }
    }
    pickMobile();
    m_systemId = detectSystemTheme();
    const auto files = QDir(":/themes").entryList({"*.json"}, QDir::Files, QDir::Name);
    for (const QString &f : files) {
        const QString id = f.left(f.size() - 5);
        m_available << id;
        QFile jf(":/themes/" + f);
        if (jf.open(QIODevice::ReadOnly)) {
            const QJsonObject o = QJsonDocument::fromJson(jf.readAll()).object();
            m_list.append(QVariantMap{{"id", id}, {"name", o.value("name").toString(id)}, {"description", o.value("description").toString()}});
        }
    }
}

// The first font of a stack that is installed ("SF Pro Text" on a Mac, Inter or Segoe UI elsewhere).
static QString firstInstalled(const QStringList &stack, const QString &fallback) {
    const QStringList have = QFontDatabase::families();
    for (const QString &f : stack) if (have.contains(f, Qt::CaseInsensitive)) return f;
    return fallback;
}

bool ThemeManager::load(const QString &id) {
    QFile f(":/themes/" + id + ".json");
    if (!f.open(QIODevice::ReadOnly)) { qWarning() << "theme not found:" << id; return false; }
    QJsonParseError err;
    const auto doc = QJsonDocument::fromJson(f.readAll(), &err);
    if (!doc.isObject()) { qWarning() << "theme" << id << "is not valid JSON:" << err.errorString(); return false; }
    m_root = doc.object().toVariantMap();
    m_id = id;
    const QString oldFamily = m_type.value("family").toString();
    m_type = m_root.value("typography").toMap();
    // family (UI text), display (large titles), mono (hashes, code) and serif (reading): the first installed font of
    // each stack, else the bundled one the theme names. "display" and "serif" default to the text family / Lora.
    if (!m_type.contains("display")) m_type.insert("display", m_type.value("family"));
    if (!m_type.contains("serif")) m_type.insert("serif", "Lora");
    if (!m_type.contains("mono")) m_type.insert("mono", "JetBrains Mono");
    for (const char *key : {"family", "display", "mono", "serif"}) {
        const QVariant stack = m_type.value(QString(key) + "Stack");
        if (stack.isValid()) m_type.insert(key, firstInstalled(stack.toStringList(), m_type.value(key).toString()));
    }
    pickColors();
    pickMobile();
    emit changed();
    applyAppFont(oldFamily);
    applyAppIcon();
    return true;
}

// The mobile metrics: what the theme's `mobile` group says, over the defaults of its family (Material 3 for Android
// and the themes without a family: 48 dp targets, 56/72 dp rows, an 80 dp navigation bar, FABs, full-screen dialogs;
// iOS HIG for the Apple family: 44 pt targets, 44/60 pt rows, a 49 pt tab bar, sheets with a grabber, no FAB).
void ThemeManager::pickMobile() {
    const bool ios = look() == "apple";
    QVariantMap d{
        {"target", ios ? 44 : 48}, {"rowH", ios ? 44 : 56}, {"row2H", ios ? 60 : 72}, {"barH", ios ? 49 : 80},
        {"fabSize", 56}, {"fabRadius", ios ? 28 : 16}, {"nav", ios ? "tab-bar" : "bar"}, {"dialog", ios ? "sheet" : "fullscreen"},
        {"grabber", ios}, {"fab", !ios}, {"minText", 14}, {"seekStep", 10}, {"navLabel", ios ? 11 : 12},
        {"appBarH", ios ? 44 : 64}, {"pageGutter", 16}, {"maxTabs", 5}
    };
    const QVariantMap own = m_root.value("mobile").toMap();
    for (auto it = own.constBegin(); it != own.constEnd(); ++it) d.insert(it.key(), it.value());
    m_mob = d;
}

// The window and taskbar icon drawn in the look of the theme (qt/icon/themes); the plain one if the theme has none.
// Set on the application, so every window that has no icon of its own (all of ours) changes at once.
void ThemeManager::applyAppIcon() {
    if (!QGuiApplication::instance()) return;
    QIcon icon;
    for (int size : {256, 512}) {
        const QString file = QString(":/icon/themes/reelhouse-%1-%2.png").arg(m_id).arg(size);
        if (QFile::exists(file)) icon.addFile(file, QSize(size, size));
    }
    if (icon.isNull()) icon = QIcon(":/icon/reelhouse-icon.png");
    QGuiApplication::setWindowIcon(icon);
}

// The default font of the program is the theme's text font, so every Text that does not name a family uses it.
// Text items made before the switch keep a copy of the old default font: they are moved to the new family by hand.
void ThemeManager::applyAppFont(const QString &oldFamily) {
    if (!QGuiApplication::instance()) return;
    const QString family = m_type.value("family").toString();
    if (family.isEmpty()) return;
    QFont f(family);
    // the rest of the stack: a glyph the first font lacks comes from the next one (Cantarell has no CJK, ...)
    QStringList fams{family};
    const QStringList have = QFontDatabase::families();
    for (const QString &s : m_type.value("familyStack").toStringList())
        if (!fams.contains(s, Qt::CaseInsensitive) && have.contains(s, Qt::CaseInsensitive)) fams << s;
    f.setFamilies(fams);
    const int body = m_type.value("body").toMap().value("size").toInt();
    if (body > 0) f.setPixelSize(body);
    QGuiApplication::setFont(f);
    if (oldFamily.isEmpty() || oldFamily.compare(family, Qt::CaseInsensitive) == 0) return;
    const auto windows = QGuiApplication::topLevelWindows();
    for (QWindow *w : windows)
        if (auto *qw = qobject_cast<QQuickWindow *>(w)) retargetFonts(qw->contentItem(), oldFamily, f.families());
}

void ThemeManager::retargetFonts(QQuickItem *item, const QString &from, const QStringList &to) {
    if (!item) return;
    if (item->inherits("QQuickText") || item->inherits("QQuickTextInput") || item->inherits("QQuickTextEdit")) {
        QFont f = item->property("font").value<QFont>();
        if (f.family().compare(from, Qt::CaseInsensitive) == 0) {
            f.setFamilies(to);
            item->setProperty("font", f);
        }
    }
    const auto kids = item->childItems();
    for (QQuickItem *k : kids) retargetFonts(k, from, to);
}

void ThemeManager::setScheme(const QString &s) {
    if (s == m_scheme) return;
    m_scheme = s;
    pickColors();
    emit changed();
}

void ThemeManager::pickColors() {
    const QVariantMap all = m_root.value("colors").toMap();
    m_colors = all.value(m_scheme).toMap();
    if (m_colors.isEmpty()) m_colors = all.value("dark").toMap();
    // The accent colour chosen in Preferences replaces the theme's, with its lighter text shade and faded fill.
    // A theme that does not define a shade (GNOME has no "fill"s) gets the nearest one, so no colour is ever undefined.
    auto fallback = [this](const char *key, const char *from) { if (!m_colors.contains(key) && m_colors.contains(from)) m_colors.insert(key, m_colors.value(from)); };
    fallback("fill", "hover"); fallback("fill2", "surface"); fallback("fill3", "view");
    fallback("label2", "textDim"); fallback("label3", "textDim"); fallback("label4", "divider");
    fallback("glassBorder", "border"); fallback("sidebarSelected", "selected");
    fallback("layer", "window"); fallback("pane", "headerbar");
    fallback("star", "rating"); fallback("starEdge", "star");
    // Material 3 roles (android.json names them all); the other themes get the nearest of their own colours
    fallback("primaryContainer", "chipBlue"); fallback("onPrimaryContainer", "chipBlueText");
    fallback("secondaryContainer", "sidebarSelected"); fallback("onSecondaryContainer", "text");
    fallback("tertiary", "accentText"); fallback("tertiaryContainer", "chip"); fallback("onTertiaryContainer", "text");
    fallback("surfaceDim", "window"); fallback("surfaceBright", "surface2");
    fallback("surfaceContainerLowest", "view"); fallback("surfaceContainerLow", "surface"); fallback("surfaceContainer", "card");
    fallback("surfaceContainerHigh", "surface2"); fallback("surfaceContainerHighest", "fill");
    fallback("outline", "border"); fallback("outlineVariant", "divider");
    fallback("inverseSurface", "text"); fallback("inverseOnSurface", "window");
    fallback("stateHover", "hover"); fallback("stateFocus", "selected"); fallback("statePressed", "selected");
    // iOS roles (ios.json names them); the others map to their window / card / divider
    fallback("groupedBg", "window"); fallback("groupedCard", "card"); fallback("separator", "divider");
    fallback("tabBar", "headerbar"); fallback("grabber", "textDim"); fallback("fab", "accent"); fallback("onFab", "accentFg");
    if (!m_colors.contains("glassHighlight")) m_colors.insert("glassHighlight", "#00ffffff");
    // Empty = "the accent colour of your system": the desktop portal (GNOME 47+, KDE, Windows, macOS) gives it to Qt.
    QColor a(m_accent);
    if (m_accent.isEmpty() && QGuiApplication::instance() && m_root.value("id").toString() == "gnome") {
        const QColor sys = QGuiApplication::palette().color(QPalette::Accent);
        if (sys.isValid() && sys.alpha() == 255) a = sys;
    }
    if (a.isValid()) {
        auto hexa = [](const QColor &c, int alpha) { return QString("#%1%2").arg(alpha, 2, 16, QLatin1Char('0')).arg(c.name().mid(1)); };
        m_colors.insert("accent", a.name());
        m_colors.insert("accentText", m_scheme == "light" ? a.darker(125).name() : a.lighter(140).name());
        m_colors.insert("accentFaded", hexa(a, 0x8a));
        m_colors.insert("chipBlue", m_scheme == "light" ? hexa(a, 0x40) : hexa(a.darker(190), 0xff));
    }
    // Text and icons on the accent: the theme's own colour, unless the accent chosen in Preferences (or the system's)
    // makes it hard to read; then white or black, whichever stands out more. Without this a dark accent under a
    // theme whose accentFg is dark (Fluent and Material dark) gave dark icons on a dark fill.
    {
        auto lum = [](const QColor &c) {
            auto ch = [](double v) { v /= 255.0; return v <= 0.03928 ? v / 12.92 : std::pow((v + 0.055) / 1.055, 2.4); };
            return 0.2126 * ch(c.red()) + 0.7152 * ch(c.green()) + 0.0722 * ch(c.blue());
        };
        auto contrast = [&lum](const QColor &x, const QColor &y) {
            const double a = lum(x), b = lum(y);
            return (std::max(a, b) + 0.05) / (std::min(a, b) + 0.05);
        };
        const QColor bg(m_colors.value("accent").toString());
        const QColor fg(m_colors.value("accentFg").toString());
        if (bg.isValid() && (!fg.isValid() || contrast(fg, bg) < 3.0)) {
            const QColor white(Qt::white), black(Qt::black);
            m_colors.insert("accentFg", contrast(white, bg) >= contrast(black, bg) * 0.75 ? white.name() : black.name());
        }
    }
    // Shades of the accent used by chips and highlights (GTK: alpha(@accent_bg_color, x)).
    const QColor acc(m_colors.value("accent").toString());
    if (acc.isValid()) {
        auto hexa = [](const QColor &c, int alpha) { return QString("#%1%2").arg(alpha, 2, 16, QLatin1Char('0')).arg(c.name().mid(1)); };
        m_colors.insert("accent20", hexa(acc, 0x33));
        m_colors.insert("accent25", hexa(acc, 0x40));
        m_colors.insert("accent28", hexa(acc, 0x47));
        m_colors.insert("accent50", hexa(acc, 0x80));
    }
    // The window colour see-through, for the scrims over wallpapers.
    const QColor w(m_colors.value("window").toString());
    if (w.isValid()) {
        auto hexa = [](const QColor &c, int alpha) { return QString("#%1%2").arg(alpha, 2, 16, QLatin1Char('0')).arg(c.name().mid(1)); };
        for (int a : {0, 45, 55, 65, 86, 95, 98}) m_colors.insert(QString("window%1").arg(a), hexa(w, a * 255 / 100));
    }
    // The application palette too: the attached ToolTips are shared popups that do not take the window's QML palette,
    // so without this they were drawn as white system boxes over every theme.
    if (QGuiApplication::instance()) {
        auto col = [this](const char *k, const char *alt) { QColor c(m_colors.value(k).toString()); return c.isValid() ? c : QColor(m_colors.value(alt).toString()); };
        QPalette p = QGuiApplication::palette();
        p.setColor(QPalette::ToolTipBase, col("popover", "window"));
        p.setColor(QPalette::ToolTipText, col("text", "text"));
        p.setColor(QPalette::Window, col("window", "window"));
        p.setColor(QPalette::WindowText, col("text", "text"));
        p.setColor(QPalette::Base, col("view", "window"));
        p.setColor(QPalette::Text, col("text", "text"));
        p.setColor(QPalette::Button, col("button", "window"));
        p.setColor(QPalette::ButtonText, col("text", "text"));
        p.setColor(QPalette::Highlight, col("accent", "accent"));
        p.setColor(QPalette::HighlightedText, col("accentFg", "text"));
        p.setColor(QPalette::PlaceholderText, col("textDim", "text"));
        p.setColor(QPalette::Dark, col("divider", "textDim"));
        p.setColor(QPalette::Mid, col("divider", "textDim"));
        s_ownPalette = true;   // our own change must not look like a desktop change (it would loop)
        QGuiApplication::setPalette(p);
        s_ownPalette = false;
    }
}

void ThemeManager::followSystem() {
    // Light/dark or accent changed in the desktop settings while we are set to follow it.
    if (m_wantScheme == "system") apply("system", m_accent);
    else if (m_accent.isEmpty()) apply(m_wantScheme, m_accent);
}

void ThemeManager::apply(const QString &scheme, const QString &accent) {
    m_wantScheme = scheme.isEmpty() ? "system" : scheme;
    m_accent = accent;
    QString s = m_wantScheme;
    if (s == "system") {
        const auto cs = QGuiApplication::styleHints()->colorScheme();
        s = cs == Qt::ColorScheme::Light ? "light" : "dark";
    }
    m_scheme = s;
    pickColors();
    emit changed();
}
