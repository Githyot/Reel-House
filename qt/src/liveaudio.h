#pragma once
// The sound of Study > Conversation by voice (Gemini Live, src/live.rs): the microphone and the tutor's voice move
// between the sound devices (Qt Multimedia) and the hub of the core on a thread of their own, so a busy window never
// makes the voice stutter. Desktop only (built when Qt Multimedia is found: REELHOUSE_LIVE_AUDIO).
namespace LiveAudio {
// GUI thread, at each tick of the core: opens the devices when a call wants them and closes them after it.
void sync();
// The window closes: the devices and the thread stop.
void shutdown();
}
