#include "liveaudio.h"
#include "bridge.h"
#include <QAudioDevice>
#include <QAudioFormat>
#include <QAudioSink>
#include <QAudioSource>
#include <QByteArray>
#include <QCoreApplication>
#include <QDebug>
#include <QIODevice>
#include <QMediaDevices>
#include <QObject>
#include <QThread>
#include <QTimer>
#include <cstdint>
#include <cstring>
#include <vector>
#if QT_VERSION >= QT_VERSION_CHECK(6, 5, 0)
#  if QT_CONFIG(permissions)
#    include <QPermissions>
#    define RH_MIC_PERMISSION 1
#  endif
#endif

// Why the sound goes through C++ (Qt Multimedia) and not a Rust crate (cpal): Qt Multimedia is already linked on the
// desktop and packed by the Windows build (MSYS2 qt6-multimedia), it opens the same devices as the rest of the window
// (WASAPI, PulseAudio/PipeWire, CoreAudio) and asks macOS for the microphone; cpal would add ALSA headers to the Linux
// build and a second audio stack. The core only sees 16-bit mono samples (rh_live_mic / rh_live_speaker).

namespace {

// The microphone as Gemini wants it (16 kHz mono 16-bit) when the device can; else its own format, converted here.
constexpr int kMicRate = 16000;
// The tutor's voice as Gemini sends it (24 kHz mono 16-bit) when the device can.
constexpr int kVoiceRate = 24000;
// How much the speaker holds: small, so an interruption stops the voice at once.
constexpr qint64 kSinkUs = 150000;
constexpr qint64 kSourceUs = 200000;

void report(const QString &text) {
    qWarning().noquote() << "[live] audio:" << text;
    const QByteArray t = text.toUtf8();
    rh_live_audio_error(t.constData());
}

QString formatName(const QAudioFormat &f) {
    const char *kind = "?";
    switch (f.sampleFormat()) {
    case QAudioFormat::UInt8: kind = "u8"; break;
    case QAudioFormat::Int16: kind = "s16"; break;
    case QAudioFormat::Int32: kind = "s32"; break;
    case QAudioFormat::Float: kind = "f32"; break;
    default: break;
    }
    return QStringLiteral("%1 Hz, %2 ch, %3").arg(f.sampleRate()).arg(f.channelCount()).arg(QLatin1String(kind));
}

QString errorName(QAudio::Error e) {
    switch (e) {
    case QAudio::OpenError: return QStringLiteral("the device could not be opened (in use, or not allowed)");
    case QAudio::IOError: return QStringLiteral("the device stopped (unplugged?)");
    case QAudio::FatalError: return QStringLiteral("the device failed");
    default: return QString();
    }
}

// Interleaved samples of any format to 16-bit mono (channels averaged).
void toMono16(const char *data, qsizetype bytes, const QAudioFormat &f, std::vector<int16_t> &out) {
    const int ch = qMax(1, f.channelCount());
    const int bps = f.bytesPerSample();
    out.clear();
    if (bps <= 0) return;
    const qsizetype frames = bytes / (qsizetype(bps) * ch);
    out.resize(size_t(frames));
    for (qsizetype i = 0; i < frames; ++i) {
        int acc = 0;
        for (int c = 0; c < ch; ++c) {
            const char *p = data + (i * ch + c) * bps;
            int v = 0;
            switch (f.sampleFormat()) {
            case QAudioFormat::UInt8: v = (int(uint8_t(*p)) - 128) * 256; break;
            case QAudioFormat::Int16: { int16_t s; std::memcpy(&s, p, 2); v = s; break; }
            case QAudioFormat::Int32: { int32_t s; std::memcpy(&s, p, 4); v = s / 65536; break; }
            case QAudioFormat::Float: { float s; std::memcpy(&s, p, 4); v = int(qBound(-1.0f, s, 1.0f) * 32767.0f); break; }
            default: break;
            }
            acc += v;
        }
        out[size_t(i)] = int16_t(acc / ch);
    }
}

// 16-bit mono to the device's format (the same sample in every channel).
void fromMono16(const int16_t *in, size_t n, const QAudioFormat &f, QByteArray &out) {
    const int ch = qMax(1, f.channelCount());
    const int bps = qMax(1, f.bytesPerSample());
    out.resize(qsizetype(n) * ch * bps);
    char *p = out.data();
    for (size_t i = 0; i < n; ++i) {
        const int16_t s = in[i];
        for (int c = 0; c < ch; ++c) {
            switch (f.sampleFormat()) {
            case QAudioFormat::UInt8: *p = char(uint8_t(s / 256 + 128)); break;
            case QAudioFormat::Int16: std::memcpy(p, &s, 2); break;
            case QAudioFormat::Int32: { const int32_t v = int32_t(s) * 65536; std::memcpy(p, &v, 4); break; }
            case QAudioFormat::Float: { const float v = float(s) / 32768.0f; std::memcpy(p, &v, 4); break; }
            default: std::memset(p, 0, size_t(bps)); break;
            }
            p += bps;
        }
    }
}

// Lives on the sound thread: opens the default microphone and speaker, then every 10 ms hands the microphone to the
// core and fills the speaker with what the core has (no Q_OBJECT: plain lambdas, no moc).
class Worker : public QObject {
public:
    void open() {
        close();
        m_lastError.clear();
        openSource();
        openSink();
        if (!m_timer) {
            m_timer = new QTimer(this);
            m_timer->setTimerType(Qt::PreciseTimer);
            QObject::connect(m_timer, &QTimer::timeout, this, [this] { tick(); });
        }
        m_timer->start(10);
    }

    void close() {
        if (m_timer) m_timer->stop();
        if (m_src) {
            m_src->stop();
            delete m_src;
            m_src = nullptr;
            m_in = nullptr;
        }
        if (m_sink) {
            m_sink->reset();
            m_sink->stop();
            delete m_sink;
            m_sink = nullptr;
            m_out = nullptr;
        }
    }

    ~Worker() override { close(); }

private:
    void fail(const QString &text) {
        // once per problem and call: the core shows it in the call and writes it to the log
        if (text == m_lastError) return;
        m_lastError = text;
        report(text);
    }

    void openSource() {
        // the microphone chosen in Conversation > Voice, else the default one
        QAudioDevice dev = QMediaDevices::defaultAudioInput();
        if (char *id = rh_live_audio_mic()) {
            const QByteArray want(id);
            rh_free_str(id);
            if (!want.isEmpty()) {
                for (const QAudioDevice &d : QMediaDevices::audioInputs()) {
                    if (d.id() == want) { dev = d; break; }
                }
            }
        }
        if (dev.isNull()) {
            fail(QStringLiteral("no microphone was found (Windows: Settings > System > Sound > Input)"));
            return;
        }
        QAudioFormat f;
        f.setSampleRate(kMicRate);
        f.setChannelCount(1);
        f.setSampleFormat(QAudioFormat::Int16);
        if (!dev.isFormatSupported(f)) f = dev.preferredFormat();
        m_inFmt = f;
        m_src = new QAudioSource(dev, f, this);
        m_src->setBufferSize(f.bytesForDuration(kSourceUs));
        QObject::connect(m_src, &QAudioSource::stateChanged, this, [this](QAudio::State) {
            if (!m_src) return;
            const QString e = errorName(m_src->error());
            if (!e.isEmpty()) fail(QStringLiteral("microphone: ") + e);
        });
        m_in = m_src->start();
        qInfo().noquote() << "[live] audio: microphone" << dev.description() << "(" + formatName(f) + ")";
        if (!m_in) fail(QStringLiteral("microphone: ") + errorName(QAudio::OpenError));
    }

    void openSink() {
        const QAudioDevice dev = QMediaDevices::defaultAudioOutput();
        if (dev.isNull()) {
            fail(QStringLiteral("no speaker or headphones were found"));
            return;
        }
        QAudioFormat f;
        f.setSampleRate(kVoiceRate);
        f.setChannelCount(1);
        f.setSampleFormat(QAudioFormat::Int16);
        if (!dev.isFormatSupported(f)) f = dev.preferredFormat();
        m_outFmt = f;
        m_sink = new QAudioSink(dev, f, this);
        m_sink->setBufferSize(f.bytesForDuration(kSinkUs));
        QObject::connect(m_sink, &QAudioSink::stateChanged, this, [this](QAudio::State) {
            if (!m_sink) return;
            // an underrun only means the tutor is quiet
            const QString e = errorName(m_sink->error());
            if (!e.isEmpty()) fail(QStringLiteral("speaker: ") + e);
        });
        m_out = m_sink->start();
        qInfo().noquote() << "[live] audio: speaker" << dev.description() << "(" + formatName(f) + ", buffer" << m_sink->bufferSize() << "bytes)";
        if (!m_out) fail(QStringLiteral("speaker: ") + errorName(QAudio::OpenError));
    }

    void tick() {
        if (m_in) {
            const QByteArray b = m_in->readAll();
            if (!b.isEmpty()) {
                toMono16(b.constData(), b.size(), m_inFmt, m_mono);
                if (!m_mono.empty()) rh_live_mic(m_mono.data(), m_mono.size(), m_inFmt.sampleRate());
            }
        }
        if (m_sink && m_out) {
            const int frameBytes = m_outFmt.bytesPerFrame();
            const qint64 freeBytes = qMax<qint64>(0, m_sink->bytesFree());
            const qint64 held = qMax<qint64>(0, m_sink->bufferSize() - freeBytes);
            const int heldMs = int(m_outFmt.durationForBytes(held) / 1000);
            const size_t cap = frameBytes > 0 ? size_t(freeBytes / frameBytes) : 0;
            m_play.resize(qMax<size_t>(cap, 1));
            int flush = 0;
            // asked even when the device is full: an interruption must be heard at once
            const size_t n = rh_live_speaker(m_play.data(), cap, m_outFmt.sampleRate(), heldMs, &flush);
            if (flush) {
                // the learner interrupted: what the device holds is dropped
                m_sink->reset();
                m_sink->stop();
                m_out = m_sink->start();
                return;
            }
            if (n > 0) {
                fromMono16(m_play.data(), n, m_outFmt, m_bytes);
                m_out->write(m_bytes);
            }
        }
    }

    QTimer *m_timer = nullptr;
    QAudioSource *m_src = nullptr;
    QIODevice *m_in = nullptr;
    QAudioFormat m_inFmt;
    QAudioSink *m_sink = nullptr;
    QIODevice *m_out = nullptr;
    QAudioFormat m_outFmt;
    std::vector<int16_t> m_mono;
    std::vector<int16_t> m_play;
    QByteArray m_bytes;
    QString m_lastError;
};

QThread *g_thread = nullptr;
Worker *g_worker = nullptr;
bool g_open = false;
bool g_asking = false;

// macOS asks the user before the microphone opens (Windows and Linux answer "granted" at once). A refusal still opens
// the speaker: the learner hears the tutor and can type.
bool micAllowed() {
#ifdef RH_MIC_PERMISSION
    QMicrophonePermission mic;
    switch (qApp->checkPermission(mic)) {
    case Qt::PermissionStatus::Undetermined:
        if (!g_asking) {
            g_asking = true;
            qApp->requestPermission(mic, qApp, [](const QPermission &) { g_asking = false; });
        }
        return false;
    case Qt::PermissionStatus::Denied:
        report(QStringLiteral("the microphone is not allowed for Reelhouse (System Settings > Privacy & Security > Microphone)"));
        return true;
    default:
        return true;
    }
#else
    return true;
#endif
}

} // namespace

void LiveAudio::sync() {
    const bool want = rh_live_audio_wanted() != 0;
    if (want == g_open) return;
    if (want) {
        // the next tick tries again once the user answered
        if (!micAllowed()) return;
        if (!g_thread) {
            g_thread = new QThread;
            g_thread->setObjectName(QStringLiteral("live-audio"));
            g_worker = new Worker;
            g_worker->moveToThread(g_thread);
            // the worker (its devices and timer) is deleted on its own thread when that ends
            QObject::connect(g_thread, &QThread::finished, g_worker, &QObject::deleteLater);
            g_thread->start(QThread::TimeCriticalPriority);
        }
        QMetaObject::invokeMethod(g_worker, [] { g_worker->open(); }, Qt::QueuedConnection);
        g_open = true;
    } else {
        QMetaObject::invokeMethod(g_worker, [] { g_worker->close(); }, Qt::QueuedConnection);
        g_open = false;
    }
}

void LiveAudio::shutdown() {
    if (!g_thread) return;
    QMetaObject::invokeMethod(g_worker, [] { g_worker->close(); }, Qt::BlockingQueuedConnection);
    g_thread->quit();
    if (g_thread->wait(2000)) {
        delete g_thread;
    }
    g_thread = nullptr;
    g_worker = nullptr;
    g_open = false;
}
