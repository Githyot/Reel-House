#include "voiceinput.h"
#include "bridge.h"

#include <QAudioDevice>
#include <QAudioSource>
#include <QDebug>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QIODevice>
#include <QJsonDocument>
#include <QJsonObject>
#include <QUrl>
#include <QVariantMap>
#include <algorithm>
#include <cmath>
#include <cstring>

// below this the device sends only zeros (the same limit as reelhouse::vad's DEAD_PEAK)
static constexpr float kDead = 2.0f / 32768.0f;

VoiceInput::VoiceInput(QObject *parent) : QObject(parent)
{
    connect(&m_devices, &QMediaDevices::audioInputsChanged, this, &VoiceInput::inputsChanged);
    m_scanTimer.setSingleShot(true);
    connect(&m_scanTimer, &QTimer::timeout, this, &VoiceInput::scanStep);
}

// The device and format that gave sound last time, tried first from then on (for this run of the app).
static QByteArray s_goodId;
static QAudioFormat s_goodFmt;

// The ways to open a microphone, best first: the one that worked before, the chosen device in its own format, then
// in 16-bit mono (48 and 16 kHz: some Windows drivers send zeros in their float format), then, with `others`, every
// other microphone of this computer the same way.
std::vector<VoiceInput::Way> VoiceInput::ways(const QAudioDevice &first, bool others)
{
    std::vector<Way> out;
    auto add = [&out](const QAudioDevice &d) {
        QList<QAudioFormat> fs;
        if (d.id() == s_goodId && s_goodFmt.isValid()) fs << s_goodFmt;
        const QAudioFormat pf = d.preferredFormat();
        if (pf.isValid() && pf.sampleFormat() != QAudioFormat::Unknown) fs << pf;
        for (int rate : {48000, 16000, 44100}) {
            QAudioFormat f;
            f.setSampleRate(rate);
            f.setChannelCount(1);
            f.setSampleFormat(QAudioFormat::Int16);
            if (d.isFormatSupported(f)) fs << f;
        }
        for (const QAudioFormat &f : fs) {
            bool dup = false;
            for (const Way &w : out) dup = dup || (w.dev.id() == d.id() && w.fmt == f);
            if (!dup) out.push_back({d, f});
        }
    };
    if (!first.isNull()) add(first);
    if (others) {
        const QAudioDevice def = QMediaDevices::defaultAudioInput();
        if (!def.isNull() && def.id() != first.id() && !loopback(def)) add(def);
        // never the computer's own sound ("Stereo Mix" and the like): it gives sound, but not the learner's voice
        for (const QAudioDevice &d : QMediaDevices::audioInputs())
            if (d.id() != first.id() && d.id() != def.id() && !loopback(d)) add(d);
    }
    return out;
}

// An input that records what the computer plays (Windows' "Stereo Mix", virtual cables), not a microphone.
bool VoiceInput::loopback(const QAudioDevice &d)
{
    const QString n = d.description().toLower();
    for (const char *k : {"stereo mix", "mixagem est", "mezcla est", "what u hear", "loopback", "cable output", "voicemeeter", "wave out", "monitor of"})
        if (n.contains(QLatin1String(k))) return true;
    return false;
}

VoiceInput::~VoiceInput()
{
    release();
}

bool VoiceInput::hasInput() const
{
    return !QMediaDevices::audioInputs().isEmpty();
}

QAudioDevice VoiceInput::pick(const QString &id)
{
    if (!id.isEmpty()) {
        const QByteArray want = id.toUtf8();
        for (const QAudioDevice &d : QMediaDevices::audioInputs()) {
            if (d.id() == want) return d;
        }
        qInfo().noquote() << "[voice] microphone: the one chosen is not plugged in, the default instead";
    }
    return QMediaDevices::defaultAudioInput();
}

QVariantList VoiceInput::inputs() const
{
    QVariantList out;
    for (const QAudioDevice &d : QMediaDevices::audioInputs()) {
        QVariantMap m;
        m.insert(QStringLiteral("id"), QString::fromUtf8(d.id()));
        m.insert(QStringLiteral("name"), d.description());
        m.insert(QStringLiteral("isDefault"), d.isDefault());
        out.append(m);
    }
    return out;
}

bool VoiceInput::start(const QString &path, const QString &options)
{
    cancel();
    const QJsonObject o = QJsonDocument::fromJson(options.toUtf8()).object();
    const QAudioDevice dev = pick(o.value(QStringLiteral("device")).toString());
    if (dev.isNull()) {
        emit failed(QStringLiteral("No microphone was found."));
        return true;
    }
    const QUrl u(path);
    m_path = u.isLocalFile() ? u.toLocalFile() : path;
    m_opts = options.toUtf8();
    m_ways = ways(dev, true);
    m_heardOnce = false;
    m_ending = false;
    m_cap = rh_voice_new(m_opts.constData());
    m_noteSeq = 0;
    {
        const QJsonObject o2 = QJsonDocument::fromJson(m_opts).object();
        qInfo().noquote() << QStringLiteral("[voice] detector: %1, margin %2 dB over the room, %3 ms of silence ends a turn, nothing after %4 ms, longest %5 ms")
                                 .arg(o2.value(QStringLiteral("auto")).toBool() ? QStringLiteral("ends by itself") : QStringLiteral("push to talk (ends on the button)"))
                                 .arg(o2.value(QStringLiteral("startDb")).toDouble(10), 0, 'f', 0)
                                 .arg(o2.value(QStringLiteral("silenceMs")).toInt()).arg(o2.value(QStringLiteral("maxWaitMs")).toInt())
                                 .arg(o2.value(QStringLiteral("maxTurnMs")).toInt());
    }
    if (!open(0)) {
        qWarning() << "[voice] microphone: no input could be opened";
        release();
        return false;
    }
    emit recordingChanged();
    return true;
}

// Opens the first way from `from` on that works; the core's recording stays.
bool VoiceInput::open(size_t from)
{
    for (size_t i = from; i < m_ways.size(); ++i) {
        const Way &w = m_ways[i];
        auto *src = new QAudioSource(w.dev, w.fmt, this);
        src->setBufferSize(w.fmt.bytesForDuration(200000));
        QIODevice *io = src->start();
        if (!io || src->error() != QAudio::NoError) {
            qWarning().noquote() << "[voice] microphone: could not open" << w.dev.description() << "format" << int(w.fmt.sampleFormat())
                                 << w.fmt.sampleRate() << "Hz, error" << int(src->error());
            src->stop();
            src->deleteLater();
            continue;
        }
        m_source = src;
        m_io = io;
        m_way = i;
        m_format = w.fmt;
        m_rawPeak = 0;
        m_rawFrames = 0;
        m_rest.clear();
        if (m_device != w.dev.description()) { m_device = w.dev.description(); emit deviceChanged(); }
        connect(m_source, &QAudioSource::stateChanged, this, [this](QAudio::State st) {
            if (!m_source || m_ending) return;
            const QAudio::Error err = m_source->error();
            if (st == QAudio::StoppedState && err != QAudio::NoError) {
                qWarning() << "[voice] microphone stopped with error" << err;
                if (m_way + 1 < m_ways.size()) { QMetaObject::invokeMethod(this, [this]() { next(); }, Qt::QueuedConnection); return; }
                release();
                emit failed(QStringLiteral("The microphone could not record. Check that Windows lets apps use it (Settings > Privacy > Microphone)."));
            }
        });
        connect(m_io, &QIODevice::readyRead, this, &VoiceInput::onData);
        qInfo().noquote() << "[voice] microphone:" << w.dev.description() << w.fmt.sampleRate() << "Hz," << w.fmt.channelCount()
                          << "ch, format" << int(w.fmt.sampleFormat()) << QStringLiteral("(way %1 of %2)").arg(i + 1).arg(m_ways.size());
        return true;
    }
    return false;
}

void VoiceInput::closeSource()
{
    if (m_source) {
        m_source->disconnect(this);
        if (m_io) m_io->disconnect(this);
        m_source->stop();
        m_source->deleteLater();
    }
    m_source = nullptr;
    m_io = nullptr;
}

// The current way gave only zeros (or stopped): the next one, with a fresh recording so no dead second is kept.
void VoiceInput::next()
{
    if (!m_source || m_ending) return;
    const QString was = m_device;
    closeSource();
    if (m_cap) rh_voice_free(m_cap);
    m_cap = rh_voice_new(m_opts.constData());
    m_noteSeq = 0;
    if (!open(m_way + 1)) {
        qWarning() << "[voice] microphone: every input gave only silence";
        m_ending = true;
        end(true);
        return;
    }
    qInfo().noquote() << "[voice] microphone: only silence from" << was << "- now" << m_device;
    emit switched(m_device);
}

void VoiceInput::onData()
{
    if (!m_io || !m_cap || m_ending) return;
    QByteArray data = m_rest + m_io->readAll();
    m_rest.clear();
    const int bps = m_format.bytesPerSample();
    const int ch = m_format.channelCount();
    if (bps <= 0 || ch <= 0 || data.isEmpty()) return;
    // whole frames only; the rest waits for the next read
    const qsizetype whole = data.size() / (bps * ch) * (bps * ch);
    if (whole < data.size()) { m_rest = data.mid(whole); data.truncate(whole); }
    if (data.isEmpty()) return;
    const qsizetype n = data.size() / bps;
    m_buf.resize(size_t(n));
    const char *p = data.constData();
    switch (m_format.sampleFormat()) {
    case QAudioFormat::UInt8:
        for (qsizetype i = 0; i < n; ++i) m_buf[size_t(i)] = (float(quint8(p[i])) - 128.f) / 128.f;
        break;
    case QAudioFormat::Int16:
        for (qsizetype i = 0; i < n; ++i) { qint16 v; std::memcpy(&v, p + i * 2, 2); m_buf[size_t(i)] = float(v) / 32768.f; }
        break;
    case QAudioFormat::Int32:
        for (qsizetype i = 0; i < n; ++i) { qint32 v; std::memcpy(&v, p + i * 4, 4); m_buf[size_t(i)] = float(double(v) / 2147483648.0); }
        break;
    case QAudioFormat::Float:
        for (qsizetype i = 0; i < n; ++i) { float v; std::memcpy(&v, p + i * 4, 4); m_buf[size_t(i)] = v; }
        break;
    default:
        return;
    }
    if (!m_heardOnce) {
        for (qsizetype i = 0; i < n; ++i) m_rawPeak = std::max(m_rawPeak, std::abs(m_buf[size_t(i)]));
        m_rawFrames += n / ch;
        if (m_rawPeak >= kDead) {
            m_heardOnce = true;
            s_goodId = m_ways[m_way].dev.id();
            s_goodFmt = m_format;
        } else if (m_rawFrames >= qint64(m_format.sampleRate()) * 5 / 2 && m_way + 1 < m_ways.size()) {
            // 2.5 s of pure zeros: a closed input, not a quiet room (USB and Bluetooth microphones may give zeros for
            // a second or more while they wake up: a shorter wait left the right microphone for a wrong one)
            QMetaObject::invokeMethod(this, [this]() { next(); }, Qt::QueuedConnection);
            return;
        }
    }
    const int ev = rh_voice_feed(m_cap, m_buf.data(), size_t(n), ch, m_format.sampleRate());
    // each decision of the detector (room noise measured, speech started, end of speech, nothing heard) in the log
    unsigned seq = 0;
    if (char *note = rh_voice_note(m_cap, &seq)) {
        if (seq != m_noteSeq) {
            m_noteSeq = seq;
            qInfo().noquote() << "[voice] detector:" << QString::fromUtf8(note);
        }
        rh_free_str(note);
    }
    const qreal lv = qreal(rh_voice_level(m_cap));
    if (qAbs(lv - m_level) > 0.01) { m_level = lv; emit levelChanged(); }
    const bool sp = rh_voice_speaking(m_cap) != 0;
    if (sp != m_speaking) { m_speaking = sp; emit speakingChanged(); }
    if (ev == 2 || ev == 3) {
        // not from inside the device's own signal: the source is deleted at the end
        m_ending = true;
        QMetaObject::invokeMethod(this, [this, ev]() { end(ev == 2); }, Qt::QueuedConnection);
    }
}

void VoiceInput::stop()
{
    if (!m_source) return;
    m_ending = true;
    end(true);
}

void VoiceInput::cancel()
{
    if (m_scanAt >= 0) {
        m_scanTimer.stop();
        closeSource();
        m_scanAt = -1;
        emit scanningChanged();
    }
    if (!m_source && !m_cap) return;
    m_ending = true;
    release();
}

void VoiceInput::end(bool save)
{
    if (!m_source && !m_cap) return;
    if (m_source) m_source->stop();
    int r = 0;
    if (save && m_cap) {
        QDir().mkpath(QFileInfo(m_path).absolutePath());
        const QByteArray file = QFile::encodeName(m_path);
        r = rh_voice_save(m_cap, file.constData());
    }
    if (m_cap) {
        // every recording says what came in, so a log tells a quiet or wrong microphone from a transcriber that wrote
        // nothing
        float secs = 0, peak = -100, gain = 1, rms = -100;
        rh_voice_stats(m_cap, &secs, &peak, &gain, &rms);
        const QString fmt = QStringLiteral("%1 Hz %2 ch format %3").arg(m_format.sampleRate()).arg(m_format.channelCount()).arg(int(m_format.sampleFormat()));
        qInfo().noquote() << QStringLiteral("[voice] microphone: %1 s from %2 (%3), peak %4 dBFS, RMS %5 dBFS, speech found: %6, raised x%7, %8")
                                 .arg(secs, 0, 'f', 1).arg(m_device, fmt).arg(peak, 0, 'f', 1).arg(rms, 0, 'f', 1)
                                 .arg(r == 1 ? QStringLiteral("yes") : QStringLiteral("no")).arg(gain, 0, 'f', 1)
                                 .arg(!save ? QStringLiteral("not saved (nobody spoke)") : r == 0 ? QStringLiteral("too short to save")
                                      : r == 3 ? QStringLiteral("only silence") : r < 0 ? QStringLiteral("save failed") : QStringLiteral("sent"));
    }
    release();
    if (r == 1 || r == 2) {
        if (r == 2) qInfo() << "[voice] microphone: no speech found by the detector, the whole recording is sent";
        emit finished(QUrl::fromLocalFile(m_path).toString());
    } else if (r == 3) {
        emit failed(QStringLiteral("The microphone gave only silence. On Windows, let desktop apps use it (Settings > Privacy & security > Microphone), or choose another microphone in Conversation > Voice."));
    } else if (r < 0) {
        emit failed(QStringLiteral("The recording could not be saved."));
    } else {
        emit nothing();
    }
}

void VoiceInput::release()
{
    const bool was = m_source != nullptr;
    closeSource();
    if (m_cap) { rh_voice_free(m_cap); m_cap = nullptr; }
    if (m_speaking) { m_speaking = false; emit speakingChanged(); }
    if (m_level != 0) { m_level = 0; emit levelChanged(); }
    if (was) emit recordingChanged();
}

// ---- scan(): each microphone (own format, then 16-bit mono) for 0.7 s ----
void VoiceInput::scan()
{
    if (m_scanAt >= 0 || recording()) return;
    m_scanWays.clear();
    for (const QAudioDevice &d : QMediaDevices::audioInputs()) {
        std::vector<Way> w = ways(d, false);
        m_scanWays.insert(m_scanWays.end(), w.begin(), w.end());
    }
    m_scanOut.clear();
    m_scanAt = 0;
    emit scanningChanged();
    m_ending = false;
    m_way = 0;
    m_ways.clear();
    scanStep();
}

void VoiceInput::scanStep()
{
    // the result of the way just heard
    if (m_source) {
        const Way &w = m_scanWays[size_t(m_scanAt)];
        const bool live = m_rawPeak >= kDead;
        const QByteArray id = w.dev.id();
        bool seen = false;
        for (QVariant &v : m_scanOut) {
            QVariantMap m = v.toMap();
            if (m.value(QStringLiteral("id")).toString() != QString::fromUtf8(id)) continue;
            seen = true;
            if (live && !m.value(QStringLiteral("live")).toBool()) {
                m.insert(QStringLiteral("live"), true);
                m.insert(QStringLiteral("peakDb"), 20.0 * std::log10(std::max(m_rawPeak, 1e-5f)));
                v = m;
            }
        }
        if (!seen) {
            QVariantMap m;
            m.insert(QStringLiteral("id"), QString::fromUtf8(id));
            m.insert(QStringLiteral("name"), w.dev.description());
            m.insert(QStringLiteral("isDefault"), w.dev.isDefault());
            m.insert(QStringLiteral("live"), live);
            m.insert(QStringLiteral("peakDb"), live ? 20.0 * std::log10(std::max(m_rawPeak, 1e-5f)) : -100.0);
            m_scanOut.append(m);
        }
        qInfo().noquote() << "[voice] scan:" << w.dev.description() << int(w.fmt.sampleFormat()) << w.fmt.sampleRate() << "Hz"
                          << (live ? "sound" : "only silence");
        if (live && s_goodId.isEmpty()) { s_goodId = id; s_goodFmt = w.fmt; }
        closeSource();
        ++m_scanAt;
    }
    while (size_t(m_scanAt) < m_scanWays.size()) {
        const Way &w = m_scanWays[size_t(m_scanAt)];
        auto *src = new QAudioSource(w.dev, w.fmt, this);
        QIODevice *io = src->start();
        if (!io || src->error() != QAudio::NoError) { src->stop(); src->deleteLater(); ++m_scanAt; continue; }
        m_source = src;
        m_io = io;
        m_format = w.fmt;
        m_rawPeak = 0;
        connect(m_io, &QIODevice::readyRead, this, [this]() {
            if (!m_io) return;
            const QByteArray d = m_io->readAll();
            const int bps = m_format.bytesPerSample();
            if (bps <= 0) return;
            const qsizetype n = d.size() / bps;
            const char *p = d.constData();
            for (qsizetype i = 0; i < n; ++i) m_rawPeak = std::max(m_rawPeak, std::abs(m_format.normalizedSampleValue(p + i * bps)));
        });
        m_scanTimer.start(700);
        return;
    }
    m_scanAt = -1;
    emit scanningChanged();
    emit scanned(m_scanOut);
}
