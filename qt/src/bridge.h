#pragma once
#include <stddef.h>
// C interface of the Rust core (qtbridge/src/lib.rs).
#ifdef __cplusplus
extern "C" {
#endif
typedef struct Bridge Bridge;
typedef void (*rh_frame_cb)(void *);
Bridge *rh_new(void);
void rh_free(Bridge *);
void rh_free_str(char *);
char *rh_cmd(Bridge *, const char *json);
char *rh_poll(Bridge *);
char *rh_state(Bridge *, const char *name);
void rh_set_frame_callback(Bridge *, rh_frame_cb, void *);
int rh_video_init(Bridge *);
// The window's OpenGL loader, asked first by mpv.
void rh_set_gl_loader(void *(*loader)(const char *name));
// Why the player could not start ("" when it did). Free with rh_free_str.
char *rh_video_error(Bridge *);
void rh_video_render(Bridge *, int fbo, int w, int h);
unsigned char *rh_decode_webp(const unsigned char *data, size_t len, int *w, int *h, size_t *out_len);
unsigned char *rh_title_cover(const char *title, size_t *out_len);
// The bytes of a picture fetched by the core's own HTTPS (when Qt's network cannot); free with rh_free_pixels.
unsigned char *rh_fetch_picture(const char *url, size_t *out_len);
void rh_free_pixels(unsigned char *p, size_t len);
// The picture of the opening / ending video in the X-Ray (QImage::Format_RGB32), or null when nothing new.
unsigned char *rh_theme_frame(Bridge *, int max_w, int max_h, int force, int *out_w, int *out_h, size_t *out_len);
// The same for the clip playing inside a Study card.
unsigned char *rh_clip_frame(Bridge *, int max_w, int max_h, int force, int *out_w, int *out_h, size_t *out_len);
void rh_mood(const unsigned char *px, size_t n, unsigned char *out);
char *rh_pad_poll(void);
// The orbs of time at `now`: 5 floats per point into out (cap floats); returns the points written (7 orbs, each the
// orb then its trail; OrbsItem).
size_t rh_orbs(double now, float *out, size_t cap);
// Controllers: the left stick deadzone; PlayStation controllers over HID (light bar, touchpad as a mouse, vibration,
// battery). rh_ds4_poll writes 10 floats: connected, kind (4/5), bluetooth, battery, charging, dx, dy, wheel, click,
// touching.
void rh_pad_deadzone(float dz);
void rh_ds4_enable(int on);
void rh_ds4_config(unsigned char r, unsigned char g, unsigned char b, int touch_mouse, float speed);
void rh_ds4_rumble(unsigned char weak, unsigned char strong, unsigned int ms);
size_t rh_ds4_poll(float *out, size_t cap);
// A made-up interface sound as a WAV (kind 0 = focus moved, 1 = select; length in *len). Free with rh_free_pixels(p, len).
unsigned char *rh_nav_blip(int kind, size_t *len);
// What this build of the core can do where it runs (JSON: os, mobile, localProcesses, browser...). Free with rh_free_str.
char *rh_platform_caps(void);
// IP protection (src/privacy.rs): the local gateway's port when the app's traffic must go through it, else 0.
int rh_privacy_port(void);
char *rh_tr(const char *lang, const char *text);
// Research mode's ad blocker: 1 when a request of a page should be cancelled (any thread; kind "script", "image"...).
int rh_adblock_check(const char *url, const char *source, const char *kind);
// Its element hiding CSS for the page at url ("" when none). Free with rh_free_str.
char *rh_adblock_css(const char *url);
// The microphone of the Study mode (reelhouse::vad): samples in, 16 kHz mono WAV out, with the learner's turns.
typedef struct RhVoiceCapture RhVoiceCapture;
// options: the JSON of the detector (Core.state("voice").vad, with "auto"). Free with rh_voice_free.
RhVoiceCapture *rh_voice_new(const char *options);
// count interleaved samples (-1..1). Returns 0 nothing new, 1 speech started, 2 the turn ended, 3 nobody spoke.
int rh_voice_feed(RhVoiceCapture *, const float *samples, size_t count, int channels, int rate);
float rh_voice_level(const RhVoiceCapture *);
int rh_voice_speaking(const RhVoiceCapture *);
// The detector's last decision for the [voice] log, and its number in seq (grows with each). Free with rh_free_str.
char *rh_voice_note(const RhVoiceCapture *, unsigned *seq);
// 1 saved with speech, 2 saved without speech found, 0 nothing recorded, 3 the device gave only silence (nothing
// written), -1 the file failed.
int rh_voice_save(RhVoiceCapture *, const char *path);
// For the log: seconds recorded, loudest sample (dBFS), gain given to the WAV file.
void rh_voice_stats(const RhVoiceCapture *, float *seconds, float *peak_db, float *gain, float *rms_db);
void rh_voice_free(RhVoiceCapture *);
int rh_ui_scale(void);
char *rh_data_dir(void);
void rh_set_debug_dir(const char *dir);
char *rh_last_step(void);
// Sound of Gemini Live (desktop only; qt/src/liveaudio.cpp). See qtbridge/src/lib.rs.
void rh_live_audio_present(int on);
int rh_live_audio_wanted(void);
// The microphone chosen in Conversation > Voice (a Qt audio input id; "" = the default). Free with rh_free_str.
char *rh_live_audio_mic(void);
void rh_live_mic(const short *pcm, size_t n, int rate);
size_t rh_live_speaker(short *out, size_t cap, int rate, int queued_ms, int *flush);
void rh_live_audio_error(const char *text);
#ifdef __cplusplus
}
#endif
