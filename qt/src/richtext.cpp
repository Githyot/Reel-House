// The rich text of the speech bubbles: formatting a selection and writing it back as a few simple tags.
#include "corebridge.h"
#include "editedpage.h"
#include <QQuickTextDocument>
#include <QAbstractTextDocumentLayout>
#include <QTextBlock>
#include <QTextLayout>
#include <QTextCharFormat>
#include <QTextCursor>
#include <QTextDocument>
#include <QTextFragment>
#include <QDateTime>
#include <QDir>
#include <QPointer>
#include <QThreadPool>
#include <QCoreApplication>

void CoreBridge::setEditOps(const QString &url, const QString &opsJson) { EditedPageProvider::setOps(url, opsJson); }
QString CoreBridge::pickColor(const QString &url, double x, double y) { return EditedPageProvider::pickColor(url, x, y); }
QRectF CoreBridge::cropOf(const QString &opsJson) { return EditedPageProvider::cropOf(opsJson); }

void CoreBridge::cutPage(const QString &url, const QRectF &r, const QString &tag) {
    // The page may have to come from the web: the work runs on the thread pool and the answer comes back as a signal
    // on the interface thread (guarded, the bridge could be gone when a slow download ends).
    QPointer<CoreBridge> self(this);
    QThreadPool::globalInstance()->start([self, url, r, tag]() {
        const QImage cut = EditedPageProvider::cutOut(url, r);
        QString path, error;
        if (cut.isNull()) {
            error = "The page could not be loaded.";
        } else {
            path = QDir::temp().filePath(QString("reelhouse-cut-%1.png").arg(QDateTime::currentMSecsSinceEpoch()));
            if (!cut.save(path, "PNG")) { error = "The picture could not be written."; path.clear(); }
        }
        QMetaObject::invokeMethod(qApp, [self, tag, path, error]() {
            if (self) emit self->pageCut(tag, path, error);
        }, Qt::QueuedConnection);
    });
}

// Highlighted words keep their colour in this property instead of the background brush: Qt paints a background as a
// square box, while the bubbles draw their highlights themselves (round "pill" ends, see richPills and BubbleText.qml).
// The saved text still uses <mark style="background:…">, so the files stay plain HTML.
static const int HighlightProp = QTextFormat::UserProperty + 7;

static QString highlightOf(const QTextCharFormat &f) {
    const QString h = f.stringProperty(HighlightProp);
    if (!h.isEmpty()) return h;
    if (f.background().style() != Qt::NoBrush) return f.background().color().name();
    return {};
}

void CoreBridge::richFormat(QQuickTextDocument *qdoc, int start, int end, const QString &kind, const QString &value) {
    if (!qdoc || !qdoc->textDocument()) return;
    QTextCursor c(qdoc->textDocument());
    if (start > end) std::swap(start, end);
    c.setPosition(start);
    c.setPosition(end, QTextCursor::KeepAnchor);
    if (!c.hasSelection()) c.select(QTextCursor::WordUnderCursor);
    const QTextCharFormat now = c.charFormat();
    QTextCharFormat f;
    if (kind == "bold") f.setFontWeight(now.fontWeight() > QFont::Normal ? QFont::Normal : QFont::Bold);
    else if (kind == "italic") f.setFontItalic(!now.fontItalic());
    else if (kind == "underline") f.setFontUnderline(!now.fontUnderline());
    else if (kind == "strike") f.setFontStrikeOut(!now.fontStrikeOut());
    else if (kind == "highlight") {
        // an empty name removes the highlight (an empty string counts as "none" everywhere)
        f.setProperty(HighlightProp, value.isEmpty() ? QString() : QColor(value).name());
        f.setBackground(Qt::NoBrush);
    } else if (kind == "color") f.setForeground(QColor(value));
    else if (kind == "clear") { c.setCharFormat(QTextCharFormat()); return; }
    c.mergeCharFormat(f);
}

QVariantMap CoreBridge::richState(QQuickTextDocument *qdoc, int pos) {
    QVariantMap m;
    if (!qdoc || !qdoc->textDocument()) return m;
    QTextCursor c(qdoc->textDocument());
    c.setPosition(qBound(0, pos, qdoc->textDocument()->characterCount() - 1));
    const QTextCharFormat f = c.charFormat();
    m["bold"] = f.fontWeight() > QFont::Normal;
    m["italic"] = f.fontItalic();
    m["underline"] = f.fontUnderline();
    m["strike"] = f.fontStrikeOut();
    m["highlight"] = !highlightOf(f).isEmpty();
    return m;
}

QString CoreBridge::richSimple(QQuickTextDocument *qdoc) {
    if (!qdoc || !qdoc->textDocument()) return {};
    QString out;
    bool firstBlock = true;
    for (QTextBlock b = qdoc->textDocument()->begin(); b.isValid(); b = b.next()) {
        if (!firstBlock) out += "<br>";
        firstBlock = false;
        for (auto it = b.begin(); !it.atEnd(); ++it) {
            const QTextFragment fr = it.fragment();
            if (!fr.isValid()) continue;
            const QTextCharFormat f = fr.charFormat();
            QString t = fr.text().toHtmlEscaped();
            t.replace(QChar::LineSeparator, "<br>");
            QString open, close;
            auto wrap = [&](const QString &o, const QString &c) { open += o; close = c + close; };
            if (f.fontWeight() > QFont::Normal) wrap("<b>", "</b>");
            if (f.fontItalic()) wrap("<i>", "</i>");
            if (f.fontUnderline()) wrap("<u>", "</u>");
            if (f.fontStrikeOut()) wrap("<s>", "</s>");
            const QString hl = highlightOf(f);
            if (!hl.isEmpty()) wrap(QString("<mark style=\"background:%1\">").arg(hl), "</mark>");
            if (f.hasProperty(QTextFormat::ForegroundBrush) && f.foreground().style() != Qt::NoBrush) {
                const QString col = f.foreground().color().name();
                if (col != "#000000") wrap(QString("<span style=\"color:%1\">").arg(col), "</span>");
            }
            out += open + t + close;
        }
    }
    return out;
}

QVariantMap CoreBridge::richPills(QQuickTextDocument *qdoc) {
    QVariantMap out;
    if (!qdoc || !qdoc->textDocument()) return out;
    QTextDocument *doc = qdoc->textDocument();

    // 1. Backgrounds that came from HTML (<mark>, a paste) become the highlight property, so Qt stops painting them as
    //    square boxes. Only when there is one: changing the document makes the text edit ask for the pills again.
    QList<QPair<int, int>> spans;
    QStringList colours;
    for (QTextBlock b = doc->begin(); b.isValid(); b = b.next())
        for (auto it = b.begin(); !it.atEnd(); ++it) {
            const QTextFragment fr = it.fragment();
            if (fr.isValid() && fr.charFormat().background().style() != Qt::NoBrush) {
                spans.append({fr.position(), fr.position() + fr.length()});
                colours.append(fr.charFormat().background().color().name());
            }
        }
    if (!spans.isEmpty()) {
        // the conversion is not something to undo (Ctrl+Z would bring the square boxes back)
        const bool undo = doc->isUndoRedoEnabled();
        doc->setUndoRedoEnabled(false);
        for (int i = 0; i < spans.size(); ++i) {
            QTextCursor c(doc);
            c.setPosition(spans[i].first);
            c.setPosition(spans[i].second, QTextCursor::KeepAnchor);
            QTextCharFormat f;
            f.setProperty(HighlightProp, colours[i]);
            f.setBackground(Qt::NoBrush);
            c.mergeCharFormat(f);
        }
        doc->setUndoRedoEnabled(undo);
    }

    // 2. One rectangle per line of every highlighted run, in document coordinates. The caller moves them to its own
    //    coordinates with the cursor rectangle of position 0, given here as (ox, oy).
    QAbstractTextDocumentLayout *dl = doc->documentLayout();
    QVariantList pills;
    bool origin = false;
    for (QTextBlock b = doc->begin(); b.isValid(); b = b.next()) {
        QTextLayout *lay = b.layout();
        if (!lay) continue;
        const QRectF br = dl->blockBoundingRect(b);   // also lays the block out when it was not yet
        if (!origin && lay->lineCount() > 0) {
            const QTextLine l0 = lay->lineAt(0);
            out["ox"] = br.x() + l0.cursorToX(0);
            out["oy"] = br.y() + l0.y();
            origin = true;
        }
        // the highlighted runs of this block (neighbouring fragments of the same colour join: bold inside a highlight)
        struct Run { int s, e; QString col; };
        QList<Run> runs;
        for (auto it = b.begin(); !it.atEnd(); ++it) {
            const QTextFragment fr = it.fragment();
            if (!fr.isValid()) continue;
            const QString col = highlightOf(fr.charFormat());
            if (col.isEmpty()) continue;
            const int s = fr.position() - b.position(), e = s + fr.length();
            if (!runs.isEmpty() && runs.last().e == s && runs.last().col == col) runs.last().e = e;
            else runs.append({s, e, col});
        }
        const QString text = b.text();
        for (const Run &r : runs) {
            for (int i = 0; i < lay->lineCount(); ++i) {
                const QTextLine line = lay->lineAt(i);
                int s = qMax(r.s, line.textStart()), e = qMin(r.e, line.textStart() + line.textLength());
                // the spaces where a line wraps are not part of the pill
                while (s < e && text.at(s).isSpace()) ++s;
                while (e > s && text.at(e - 1).isSpace()) --e;
                if (s >= e) continue;
                const qreal x0 = line.cursorToX(s), x1 = line.cursorToX(e);
                QVariantMap p;
                p["x"] = br.x() + qMin(x0, x1);
                p["y"] = br.y() + line.y();
                p["w"] = qAbs(x1 - x0);
                p["h"] = line.height();
                p["c"] = r.col;
                pills.append(p);
            }
        }
    }
    out["pills"] = pills;
    return out;
}
