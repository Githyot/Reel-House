#pragma once
// The microphone of the Study mode (Conversation), desktop only: reads the default input device with QAudioSource
// in its own format and hands the samples to the core (rh_voice_*: reelhouse::vad), which mixes them down to 16 kHz
// mono, follows the loudness and tells when the learner starts and stops talking. With `auto` in the options (every
// turn of Conversation) the recording ends by itself after the learner's pause and only the speech is written to the
// WAV file; each decision of the detector is written to the log as a "[voice] detector:" line.
// Used by MicRecorder.qml; built only when Qt Multimedia is there (REELHOUSE_VOICE_INPUT).
#include <QObject>
#include <QAudioDevice>
#include <QAudioFormat>
#include <QMediaDevices>
#include <QString>
#include <QTimer>
#include <QVariantList>
#include <vector>

class QAudioSource;
class QIODevice;
struct RhVoiceCapture; // the core's recording (bridge.h)

class VoiceInput : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool recording READ recording NOTIFY recordingChanged)
    Q_PROPERTY(bool speaking READ speaking NOTIFY speakingChanged)
    Q_PROPERTY(qreal level READ level NOTIFY levelChanged)
    Q_PROPERTY(bool hasInput READ hasInput NOTIFY inputsChanged)
public:
    explicit VoiceInput(QObject *parent = nullptr);
    ~VoiceInput() override;

    bool recording() const { return m_source != nullptr && m_scanAt < 0; }
    bool speaking() const { return m_speaking; }
    qreal level() const { return m_level; }
    bool hasInput() const;

    // Starts recording to `path` (a file path or a file: URL). `options`: the JSON of the detector
    // (Core.state("voice").vad with `auto`, and `device`: the id of the input chosen, "" = the default). False when the
    // device could not be opened (the page may use Qt's own recorder instead); a missing microphone is reported
    // through failed().
    Q_INVOKABLE bool start(const QString &path, const QString &options);
    // The microphones of this computer: [{id, name, isDefault}] (the id is what `device` takes).
    Q_INVOKABLE QVariantList inputs() const;
    // The input with this id, or the default one when it is "" or not plugged in.
    static QAudioDevice pick(const QString &id);
    // Ends now and writes the file (push to talk).
    Q_INVOKABLE void stop();
    // Ends without a file.
    Q_INVOKABLE void cancel();
    // Listens to every microphone for a moment, one after the other, and tells which ones give sound:
    // scanned([{id, name, isDefault, live, peakDb, format}]). Nothing to say: a working microphone always hears
    // the room; a closed one gives only zeros.
    Q_INVOKABLE void scan();
    Q_PROPERTY(bool scanning READ scanning NOTIFY scanningChanged)
    bool scanning() const { return m_scanAt >= 0; }
    // The name of the input recording now (it changes when the first one gave only silence)
    Q_PROPERTY(QString device READ device NOTIFY deviceChanged)
    QString device() const { return m_device; }

signals:
    void recordingChanged();
    void speakingChanged();
    void levelChanged();
    void inputsChanged();
    // the WAV file is written (a file: URL)
    void finished(const QString &url);
    void failed(const QString &text);
    // the call waited and nobody spoke, or the recording held no sound
    void nothing();
    // the chosen input gave only zeros, so the recording moved to `name` (another format or another microphone)
    void switched(const QString &name);
    void scanned(const QVariantList &results);
    void scanningChanged();
    void deviceChanged();

private:
    // one way to open a microphone: the device and the format asked of it
    struct Way { QAudioDevice dev; QAudioFormat fmt; };
    static std::vector<Way> ways(const QAudioDevice &first, bool others);
    static bool loopback(const QAudioDevice &d);
    bool open(size_t from);
    void closeSource();
    void next();
    void scanStep();
    void onData();
    void end(bool save);
    void release();

    QMediaDevices m_devices;
    QAudioSource *m_source = nullptr;
    QIODevice *m_io = nullptr;
    QAudioFormat m_format;
    RhVoiceCapture *m_cap = nullptr;
    QString m_path;
    QString m_device;
    bool m_speaking = false;
    bool m_ending = false;
    qreal m_level = 0;
    std::vector<float> m_buf;
    // the bytes of an incomplete frame at the end of a read, kept for the next one (a read may end mid-sample: dropping
    // them would shift every later sample and mix up the channels)
    QByteArray m_rest;
    // the fallback: the ways still to try, where we are, and how much silence the current one gave
    std::vector<Way> m_ways;
    size_t m_way = 0;
    float m_rawPeak = 0;
    qint64 m_rawFrames = 0;
    bool m_heardOnce = false;
    QByteArray m_opts;
    // the number of the detector's last decision written to the log
    unsigned m_noteSeq = 0;
    // scan()
    std::vector<Way> m_scanWays;
    int m_scanAt = -1;
    QVariantList m_scanOut;
    QTimer m_scanTimer;
};
