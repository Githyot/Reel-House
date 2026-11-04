#pragma once
#include <QObject>
#include <QVariantMap>
#include <QStringList>
#include <QVariantList>
#include <QPointer>
#include <QQuickItem>

// Reads a theme (a JSON file of design tokens) and gives the tokens to QML as `Theme.c`, `Theme.shape`, ...
// Pages never hard-code a colour, size or duration: they only read these.
class ThemeManager : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString id READ id NOTIFY changed)
    Q_PROPERTY(QString scheme READ scheme WRITE setScheme NOTIFY changed)
    Q_PROPERTY(QStringList available READ available CONSTANT)
    Q_PROPERTY(QVariantMap c READ colors NOTIFY changed)
    Q_PROPERTY(QVariantMap shape READ shape NOTIFY changed)
    Q_PROPERTY(QVariantMap elevation READ elevation NOTIFY changed)
    Q_PROPERTY(QVariantMap material READ material NOTIFY changed)
    Q_PROPERTY(QVariantMap type READ type NOTIFY changed)
    Q_PROPERTY(QVariantMap spacing READ spacing NOTIFY changed)
    Q_PROPERTY(QVariantMap motion READ motion NOTIFY changed)
    Q_PROPERTY(QVariantMap layout READ layout NOTIFY changed)
    Q_PROPERTY(QString name READ name NOTIFY changed)
    // true when the theme's panels are blurred translucent glass (Apple materials) and the navigation is a sidebar
    // the icon set ("adwaita" filled symbols or "lucide" outlines) and the stroke width of outline icons
    Q_PROPERTY(QString icons READ icons NOTIFY changed)
    Q_PROPERTY(double iconStroke READ iconStroke NOTIFY changed)
    Q_PROPERTY(bool glass READ glass NOTIFY changed)
    Q_PROPERTY(bool sidebarNav READ sidebarNav NOTIFY changed)
    // The item that holds the pages: glass panels blur what is under them by reading it (set by Main.qml).
    Q_PROPERTY(QQuickItem *backdrop READ backdrop WRITE setBackdrop NOTIFY backdropChanged)
    // [{id, name, description}] of every theme file
    Q_PROPERTY(QVariantList list READ list CONSTANT)
    // The family of the look ("apple" for the Apple and iOS themes, "android", "windows", "gnome"): what the components
    // switch on when they draw a platform's own control. A theme file sets `look`; without it the id is used.
    Q_PROPERTY(QString look READ look NOTIFY changed)
    // The theme of this device when the preferences name none: "hyperos" on a Xiaomi / Redmi / POCO phone (HyperOS),
    // "android" on other Android phones, "apple" on an iPhone or iPad, "gnome" on a desktop. REELHOUSE_THEME overrides it.
    Q_PROPERTY(QString systemId READ systemId CONSTANT)
    // ---- mobile (Material 3 / iOS HIG on phones and tablets) ----
    // mobile: the pages lay themselves out for a phone (bottom navigation, full-screen dialogs, 44/48 px targets,
    // no blur). Main.qml sets it from the platform (Android, iOS) or a narrow window; a Platform singleton can feed
    // it later. touch: a touch screen is present (hover affordances get a tap/long-press equivalent).
    Q_PROPERTY(bool mobile READ mobile WRITE setMobile NOTIFY mobileChanged)
    Q_PROPERTY(bool touch READ touch WRITE setTouch NOTIFY mobileChanged)
    // the safe area of the screen (notch, home indicator): {top, bottom, left, right} in px, 0 on desktops
    Q_PROPERTY(QVariantMap safe READ safe WRITE setSafe NOTIFY mobileChanged)
    // the mobile metrics of the theme (`mobile` group of the JSON, with defaults for every theme): target, rowH, row2H,
    // barH, fabSize, fabRadius, nav (bar | tab-bar), dialog (fullscreen | sheet), grabber, fab, minText, seekStep
    Q_PROPERTY(QVariantMap mob READ mob NOTIFY changed)
public:
    explicit ThemeManager(QObject *parent = nullptr);
    Q_INVOKABLE bool load(const QString &id);
    // "system", "light" or "dark", and an accent colour ("" = the theme's own), from the preferences.
    Q_INVOKABLE void apply(const QString &scheme, const QString &accent);
    void followSystem();
    QString id() const { return m_id; }
    QString scheme() const { return m_scheme; }
    void setScheme(const QString &s);
    QStringList available() const { return m_available; }
    QString systemId() const { return m_systemId; }
    QVariantMap colors() const { return m_colors; }
    QVariantMap shape() const { return m_root.value("shape").toMap(); }
    QVariantMap elevation() const { return m_root.value("elevation").toMap(); }
    QVariantMap material() const { return m_root.value("material").toMap(); }
    QVariantMap type() const { return m_type; }
    QString name() const { return m_root.value("name").toString(); }
    QString icons() const { return m_root.value("icons").toString(); }
    double iconStroke() const { return m_root.value("iconStroke", 2.0).toDouble(); }
    bool glass() const { return m_root.value("material").toMap().value("kind").toString() == "glass"; }
    bool sidebarNav() const { return m_root.value("layout").toMap().value("nav").toString() == "sidebar" || m_root.value("layout").toMap().value("nav").toString() == "rail"; }
    QQuickItem *backdrop() const { return m_backdrop; }
    void setBackdrop(QQuickItem *i) { if (m_backdrop != i) { m_backdrop = i; emit backdropChanged(); } }
    QVariantList list() const { return m_list; }
    QVariantMap spacing() const { return m_root.value("spacing").toMap(); }
    QVariantMap motion() const { return m_root.value("motion").toMap(); }
    QVariantMap layout() const { return m_root.value("layout").toMap(); }
    QString look() const { const QString l = m_root.value("look").toString(); return l.isEmpty() ? m_id : l; }
    bool mobile() const { return m_mobile; }
    void setMobile(bool m);
    bool touch() const { return m_touch; }
    void setTouch(bool t) { if (m_touch != t) { m_touch = t; emit mobileChanged(); } }
    QVariantMap safe() const { return m_safe; }
    void setSafe(const QVariantMap &s) { if (m_safe != s) { m_safe = s; emit mobileChanged(); } }
    QVariantMap mob() const { return m_mob; }
signals:
    void changed();
    void mobileChanged();
    void backdropChanged();
private:
    QString m_accent;
    QString m_wantScheme = "dark";
    void pickColors();
    void applyAppFont(const QString &oldFamily);
    void applyAppIcon();
    static void retargetFonts(QQuickItem *item, const QString &from, const QStringList &to);
    QString m_id, m_scheme = "dark", m_systemId = "gnome";
    static QString detectSystemTheme();
    static QString canonicalId(const QString &id);
    void rebuild();
    QStringList m_available;
    QVariantMap m_base, m_root, m_colors, m_type, m_mob, m_safe{{"top", 0}, {"bottom", 0}, {"left", 0}, {"right", 0}};
    bool m_mobile = false, m_touch = false;
    void pickMobile();
    QVariantList m_list;
    QPointer<QQuickItem> m_backdrop;
};
