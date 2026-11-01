//! PlayStation controllers over HID (DualShock 4 and DualSense, USB and Bluetooth), desktop only: the touchpad as a
//! mouse, the light bar colour, a short vibration, the battery, and the buttons and sticks. Once this side talks to the
//! controller over Bluetooth it switches to the full reports (0x11 / 0x31), which Windows' own gamepad driver (and so
//! gilrs) cannot read any more: so while it is connected the buttons come from here and lib.rs drops gilrs' events of
//! Sony pads. Written from the published HID report layouts (the DS4 USB report 0x01 / Bluetooth 0x11 and
//! output 0x05 / 0x11; the DualSense USB 0x01 / Bluetooth 0x31 and output 0x02 / 0x31, Bluetooth outputs closed by a
//! CRC-32 seeded with 0xA2), the same layouts the Linux hid-playstation driver and DS4Windows use; no code is copied.
//!
//! A thread ("pad-hid") finds the first Sony controller, reads its reports and writes the light bar and vibration when
//! they change. It starts with the first `set_enabled(true)` and sleeps while disabled (no controller mode).

#![cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const SONY: u16 = 0x054c;
const DS4: [u16; 3] = [0x05c4, 0x09cc, 0x0ba0];
const DS5: [u16; 2] = [0x0ce6, 0x0df2];

/// Touchpad size in its own units (DS4 1920x942, DualSense 1920x1080 — close enough for relative moves).
const PAD_W: f32 = 1920.0;

#[derive(Default)]
struct Shared {
    enabled: bool,
    connected: bool,
    /// 4 = DualShock 4, 5 = DualSense
    kind: u8,
    bt: bool,
    /// 0-100, -1 unknown
    battery: i32,
    charging: bool,
    // pointer movement since the last poll, in screen pixels, and the wheel (positive = up)
    dx: f32,
    dy: f32,
    scroll: f32,
    click: bool,
    touching: bool,
    touch_mouse: bool,
    speed: f32,
    light: [u8; 3],
    rumble: (u8, u8),
    rumble_until: Option<Instant>,
    /// the output report must be written again (light or vibration changed, or a new connection)
    dirty: bool,
    /// the controls of the last report, and the buttons pressed since the last `controls()` (so a tap between two
    /// polls is not lost)
    pad: Controls,
    latch: u32,
}

fn shared() -> &'static Mutex<Shared> {
    static S: OnceLock<Mutex<Shared>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Shared { battery: -1, touch_mouse: true, speed: 1.0, light: [0, 0, 64], ..Default::default() }))
}

static STARTED: AtomicBool = AtomicBool::new(false);

/// Turns the HID side on or off (controller mode on and a PlayStation controller expected).
pub fn set_enabled(on: bool) {
    if let Ok(mut s) = shared().lock() {
        s.enabled = on;
    }
    if on && !STARTED.swap(true, Ordering::SeqCst) {
        let _ = std::thread::Builder::new().name("pad-hid".into()).spawn(run);
    }
}

/// The light bar colour (already scaled by the brightness).
pub fn set_light(rgb: [u8; 3]) {
    if let Ok(mut s) = shared().lock() {
        if s.light != rgb {
            s.light = rgb;
            s.dirty = true;
        }
    }
}

/// A vibration of `ms` milliseconds: `weak` the small (right) motor, `strong` the big one.
pub fn rumble(weak: u8, strong: u8, ms: u32) {
    if let Ok(mut s) = shared().lock() {
        s.rumble = (weak, strong);
        s.rumble_until = Some(Instant::now() + Duration::from_millis(ms as u64));
        s.dirty = true;
    }
}

/// Touchpad as a mouse on or off, and its speed (1.0 = normal).
pub fn set_touch(on: bool, speed: f32) {
    if let Ok(mut s) = shared().lock() {
        s.touch_mouse = on;
        s.speed = speed.clamp(0.1, 5.0);
    }
}

/// [connected, kind, bluetooth, battery, charging, dx, dy, scroll, click, touching]; the movement is taken.
pub fn poll() -> [f32; 10] {
    let Ok(mut s) = shared().lock() else { return [0.0; 10] };
    let out = [
        s.connected as u8 as f32,
        s.kind as f32,
        s.bt as u8 as f32,
        s.battery as f32,
        s.charging as u8 as f32,
        s.dx,
        s.dy,
        s.scroll,
        s.click as u8 as f32,
        s.touching as u8 as f32,
    ];
    s.dx = 0.0;
    s.dy = 0.0;
    s.scroll = 0.0;
    out
}

/// The controls while a controller is connected here: the state now, and the buttons pressed since the last call
/// (taken). None when no Sony controller is open (gilrs gives the buttons then).
pub fn controls_now() -> Option<(Controls, u32)> {
    let Ok(mut s) = shared().lock() else { return None };
    if !s.connected || !s.enabled {
        return None;
    }
    let latch = s.latch;
    s.latch = 0;
    Some((s.pad, latch))
}

// ---------------------------------------------------------------------------------------------------------------
// reports (pure functions, tested below)

/// What one input report says.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Input {
    pub click: bool,
    /// up to two fingers: (x, y)
    pub touch: [Option<(u16, u16)>; 2],
    pub battery: i32,
    pub charging: bool,
    pub controls: Controls,
}

/// Buttons (the `B_*` bits) and axes (-1..1, y up positive; triggers 0..1) of one report.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Controls {
    pub buttons: u32,
    pub lx: f32,
    pub ly: f32,
    /// right stick, -1 left .. 1 right
    pub rx: f32,
    pub ry: f32,
    pub l2: f32,
    pub r2: f32,
}

pub const B_CROSS: u32 = 1 << 0;
pub const B_CIRCLE: u32 = 1 << 1;
pub const B_SQUARE: u32 = 1 << 2;
pub const B_TRIANGLE: u32 = 1 << 3;
pub const B_L1: u32 = 1 << 4;
pub const B_R1: u32 = 1 << 5;
pub const B_SHARE: u32 = 1 << 6;
pub const B_OPTIONS: u32 = 1 << 7;
pub const B_L3: u32 = 1 << 8;
pub const B_R3: u32 = 1 << 9;
pub const B_UP: u32 = 1 << 10;
pub const B_DOWN: u32 = 1 << 11;
pub const B_LEFT: u32 = 1 << 12;
pub const B_RIGHT: u32 = 1 << 13;

/// The names the interface knows (the same as lib.rs gives for gilrs), in bit order.
pub const NAMES: [&str; 14] = ["A", "B", "X", "Y", "LB", "RB", "Select", "Start", "L3", "R3", "Up", "Down", "Left", "Right"];

/// Face buttons and hat in one byte (low nibble hat 0 = up, clockwise, 8 = none; bits 4-7 square, cross, circle,
/// triangle), shoulders etc. in the next, the same in DS4 and DualSense reports.
fn controls(face: u8, sh: u8, lx: u8, ly: u8, rx: u8, ry: u8, l2: u8, r2: u8) -> Controls {
    let mut b = 0u32;
    if face & 0x10 != 0 { b |= B_SQUARE; }
    if face & 0x20 != 0 { b |= B_CROSS; }
    if face & 0x40 != 0 { b |= B_CIRCLE; }
    if face & 0x80 != 0 { b |= B_TRIANGLE; }
    b |= match face & 0x0f {
        0 => B_UP,
        1 => B_UP | B_RIGHT,
        2 => B_RIGHT,
        3 => B_DOWN | B_RIGHT,
        4 => B_DOWN,
        5 => B_DOWN | B_LEFT,
        6 => B_LEFT,
        7 => B_UP | B_LEFT,
        _ => 0,
    };
    if sh & 0x01 != 0 { b |= B_L1; }
    if sh & 0x02 != 0 { b |= B_R1; }
    if sh & 0x10 != 0 { b |= B_SHARE; }
    if sh & 0x20 != 0 { b |= B_OPTIONS; }
    if sh & 0x40 != 0 { b |= B_L3; }
    if sh & 0x80 != 0 { b |= B_R3; }
    let axis = |v: u8| (v as f32 - 127.5) / 127.5;
    Controls { buttons: b, lx: axis(lx), ly: -axis(ly), rx: axis(rx), ry: -axis(ry), l2: l2 as f32 / 255.0, r2: r2 as f32 / 255.0 }
}

fn finger(b: &[u8], at: usize) -> Option<(u16, u16)> {
    let c = *b.get(at)?;
    if c & 0x80 != 0 {
        return None;
    }
    let x = b[at + 1] as u16 | ((b[at + 2] as u16 & 0x0f) << 8);
    let y = (b[at + 2] as u16 >> 4) | ((b[at + 3] as u16) << 4);
    Some((x, y))
}

/// Parses an input report (with its id in byte 0). `ds5`: a DualSense. None for a report without touch data.
pub fn parse(b: &[u8], ds5: bool) -> Option<Input> {
    let id = *b.first()?;
    if !ds5 {
        // DS4: USB 0x01 (64 bytes); Bluetooth 0x11 is the same data two bytes later
        let s = match (id, b.len()) {
            (0x01, n) if n >= 43 => 0,
            (0x11, n) if n >= 45 => 2,
            _ => return None,
        };
        let bat = b[30 + s];
        let cable = bat & 0x10 != 0;
        let level = (bat & 0x0f) as i32;
        Some(Input {
            click: b[7 + s] & 0x02 != 0,
            touch: [finger(b, 35 + s), finger(b, 39 + s)],
            battery: if cable { (level * 10).min(100) } else { (level * 10 + 5).min(100) },
            charging: cable && level < 11,
            controls: controls(b[5 + s], b[6 + s], b[1 + s], b[2 + s], b[3 + s], b[4 + s], b[8 + s], b[9 + s]),
        })
    } else {
        // DualSense: USB 0x01 (64 bytes); Bluetooth 0x31 one byte later
        let s = match (id, b.len()) {
            (0x01, n) if n >= 55 => 0,
            (0x31, n) if n >= 56 => 1,
            _ => return None,
        };
        let bat = b[53 + s];
        Some(Input {
            click: b[10 + s] & 0x02 != 0,
            touch: [finger(b, 33 + s), finger(b, 37 + s)],
            battery: ((bat & 0x0f) as i32 * 10 + 5).min(100),
            charging: (bat >> 4) == 1,
            controls: controls(b[8 + s], b[9 + s], b[1 + s], b[2 + s], b[3 + s], b[4 + s], b[5 + s], b[6 + s]),
        })
    }
}

/// CRC-32 (IEEE) of the Bluetooth output reports: seeded with the HID header byte 0xA2.
pub fn bt_crc(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in std::iter::once(&0xa2u8).chain(data.iter()) {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// The output report: light bar `rgb`, motors `weak` (right) and `strong` (left). `seq`: DualSense Bluetooth counter.
pub fn output(ds5: bool, bt: bool, rgb: [u8; 3], weak: u8, strong: u8, seq: u8) -> Vec<u8> {
    let mut o;
    if !ds5 {
        if bt {
            o = vec![0u8; 78];
            o[0] = 0x11;
            o[1] = 0xc0; // HID + CRC
            o[3] = 0x07; // vibration, light bar, flash
            o[6] = weak;
            o[7] = strong;
            o[8..11].copy_from_slice(&rgb);
        } else {
            o = vec![0u8; 32];
            o[0] = 0x05;
            o[1] = 0x07;
            o[4] = weak;
            o[5] = strong;
            o[6..9].copy_from_slice(&rgb);
        }
    } else {
        // the common part starts at 1 on USB, at 3 on Bluetooth (after the sequence and the tag)
        let c = if bt { 3 } else { 1 };
        o = vec![0u8; if bt { 78 } else { 48 }];
        if bt {
            o[0] = 0x31;
            o[1] = (seq & 0x0f) << 4;
            o[2] = 0x10;
        } else {
            o[0] = 0x02;
        }
        o[c] = 0x01 | 0x02; // vibration (compatible mode)
        o[c + 1] = 0x04; // light bar
        o[c + 2] = weak;
        o[c + 3] = strong;
        o[c + 38] = 0x02; // light bar setup: let the app drive it
        o[c + 41] = 0x02; // fade the default blue out
        o[c + 44..c + 47].copy_from_slice(&rgb);
    }
    if bt {
        let n = o.len();
        let crc = bt_crc(&o[..n - 4]);
        o[n - 4..].copy_from_slice(&crc.to_le_bytes());
    }
    o
}

/// Moves of the touchpad, from one report to the next: one finger moves the pointer, two scroll.
#[derive(Default)]
pub struct Touch {
    last: [Option<(u16, u16)>; 2],
}

impl Touch {
    /// (dx, dy, scroll) for this report, in touchpad units (scroll positive = up).
    pub fn step(&mut self, now: [Option<(u16, u16)>; 2]) -> (f32, f32, f32) {
        let mut r = (0.0, 0.0, 0.0);
        match (now, self.last) {
            ([Some(a), Some(b)], [Some(pa), Some(pb)]) => {
                let dy = ((a.1 as f32 + b.1 as f32) - (pa.1 as f32 + pb.1 as f32)) / 2.0;
                r.2 = dy;
            }
            ([Some(a), None], [Some(p), None]) => {
                r.0 = a.0 as f32 - p.0 as f32;
                r.1 = a.1 as f32 - p.1 as f32;
            }
            _ => {}
        }
        self.last = now;
        r
    }
}

// ---------------------------------------------------------------------------------------------------------------
// the thread

#[cfg(any(windows, target_os = "macos"))]
fn run() {
    let mut api = None;
    let mut seq = 0u8;
    loop {
        let enabled = shared().lock().map(|s| s.enabled).unwrap_or(false);
        if !enabled {
            std::thread::sleep(Duration::from_millis(500));
            continue;
        }
        if api.is_none() {
            api = hidapi::HidApi::new().ok();
            if api.is_none() {
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }
        }
        let Some(hid) = api.as_mut() else { continue };
        let _ = hid.refresh_devices();
        let found = hid
            .device_list()
            .find(|d| d.vendor_id() == SONY && (DS4.contains(&d.product_id()) || DS5.contains(&d.product_id())))
            .map(|d| (d.path().to_owned(), DS5.contains(&d.product_id()), matches!(d.bus_type(), hidapi::BusType::Bluetooth)));
        let Some((path, ds5, bt_hint)) = found else {
            if let Ok(mut s) = shared().lock() {
                s.connected = false;
            }
            std::thread::sleep(Duration::from_secs(2));
            continue;
        };
        let Ok(dev) = hid.open_path(&path) else {
            std::thread::sleep(Duration::from_secs(2));
            continue;
        };
        // Bluetooth: asking a feature report switches the controller to the full reports (0x11 / 0x31)
        if bt_hint {
            let mut f = [0u8; 80];
            f[0] = if ds5 { 0x05 } else { 0x02 };
            let _ = dev.get_feature_report(&mut f);
        }
        if let Ok(mut s) = shared().lock() {
            s.connected = true;
            s.kind = if ds5 { 5 } else { 4 };
            s.bt = bt_hint;
            s.dirty = true;
        }
        let mut touch = Touch::default();
        let mut buf = [0u8; 128];
        loop {
            let n = match dev.read_timeout(&mut buf, 20) {
                Ok(n) => n,
                Err(_) => break,
            };
            let mut write = None;
            if let Ok(mut s) = shared().lock() {
                if !s.enabled {
                    break;
                }
                if n > 0 {
                    if !s.bt && (buf[0] == 0x11 || buf[0] == 0x31) {
                        s.bt = true;
                        s.dirty = true;
                    }
                    if let Some(i) = parse(&buf[..n], ds5) {
                        s.battery = i.battery;
                        s.charging = i.charging;
                        s.click = i.click;
                        s.touching = i.touch[0].is_some();
                        s.latch |= i.controls.buttons & !s.pad.buttons;
                        s.pad = i.controls;
                        let (dx, dy, sc) = touch.step(i.touch);
                        if s.touch_mouse {
                            // a full swipe across the pad is about 900 screen pixels at speed 1
                            let k = 900.0 / PAD_W * s.speed;
                            s.dx += dx * k;
                            s.dy += dy * k;
                            s.scroll -= sc * 2.0 * s.speed;
                        }
                    }
                }
                if let Some(t) = s.rumble_until {
                    if Instant::now() >= t {
                        s.rumble = (0, 0);
                        s.rumble_until = None;
                        s.dirty = true;
                    }
                }
                if s.dirty {
                    s.dirty = false;
                    seq = seq.wrapping_add(1);
                    write = Some(output(ds5, s.bt, s.light, s.rumble.0, s.rumble.1, seq));
                }
            }
            if let Some(o) = write {
                let _ = dev.write(&o);
            }
        }
        if let Ok(mut s) = shared().lock() {
            s.connected = false;
            s.touching = false;
            s.click = false;
            s.pad = Controls::default();
            s.latch = 0;
        }
    }
}

// Linux, Android, iOS: no HID access here (Linux would need the libudev headers, like gilrs).
#[cfg(not(any(windows, target_os = "macos")))]
fn run() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ds4_usb_touch_and_battery() {
        let mut b = [0u8; 64];
        b[0] = 0x01;
        b[7] = 0x02; // touchpad click
        b[30] = 0x10 | 5; // cable, level 5
        b[35] = 0x01; // finger 0 down (bit 7 clear)
        // x = 0x234, y = 0x156
        b[36] = 0x34;
        b[37] = 0x62;
        b[38] = 0x15;
        b[39] = 0x80; // finger 1 up
        let i = parse(&b, false).unwrap();
        assert!(i.click);
        assert_eq!(i.touch, [Some((0x234, 0x156)), None]);
        assert_eq!(i.battery, 50);
        assert!(i.charging);
        // the same on Bluetooth is two bytes later
        let mut bt = [0u8; 78];
        bt[0] = 0x11;
        bt[2..66].copy_from_slice(&b[..64]);
        bt[2] = 0;
        let j = parse(&bt, false).unwrap();
        assert_eq!(j.touch, i.touch);
        assert!(j.click);
    }

    #[test]
    fn buttons_and_sticks() {
        // DS4 USB: cross + D-pad right, R1 + options, left stick full left, right stick up, R2 full
        let mut b = [0u8; 64];
        b[0] = 0x01;
        b[1] = 0;
        b[2] = 128;
        b[3] = 255;
        b[4] = 0;
        b[5] = 0x20 | 2;
        b[6] = 0x02 | 0x20;
        b[9] = 255;
        let c = parse(&b, false).unwrap().controls;
        assert_eq!(c.buttons, B_CROSS | B_RIGHT | B_R1 | B_OPTIONS);
        assert!(c.lx < -0.99 && c.ly.abs() < 0.01 && c.rx > 0.99 && c.ry > 0.99 && c.r2 > 0.99);
        // hat 8 = nothing, DS4 Bluetooth two bytes later
        let mut bt = [0u8; 78];
        bt[0] = 0x11;
        bt[7] = 0x80 | 8;
        assert_eq!(parse(&bt, false).unwrap().controls.buttons, B_TRIANGLE);
        // DualSense USB: square + up-left, L1 + share at 8 / 9
        let mut d = [0u8; 64];
        d[0] = 0x01;
        d[8] = 0x10 | 7;
        d[9] = 0x01 | 0x10;
        assert_eq!(parse(&d, true).unwrap().controls.buttons, B_SQUARE | B_UP | B_LEFT | B_L1 | B_SHARE);
        // right stick X: DS4 byte 3 (Bluetooth 5), DualSense byte 3 (Bluetooth 4)
        bt[5] = 0;
        assert!(parse(&bt, false).unwrap().controls.rx < -0.99);
        let mut d5 = [0u8; 78];
        d5[0] = 0x31;
        d5[4] = 255;
        d5[9] = 8;
        assert!(parse(&d5, true).unwrap().controls.rx > 0.99);
    }

    #[test]
    fn ds5_usb_touch() {
        let mut b = [0u8; 64];
        b[0] = 0x01;
        b[10] = 0x02;
        b[33] = 0x80;
        b[37] = 0x03;
        b[38] = 0x10;
        b[39] = 0x00;
        b[40] = 0x01;
        b[53] = 0x18; // charging, level 8
        let i = parse(&b, true).unwrap();
        assert!(i.click);
        assert_eq!(i.touch, [None, Some((0x010, 0x010))]);
        assert_eq!(i.battery, 85);
        assert!(i.charging);
    }

    #[test]
    fn outputs() {
        let o = output(false, false, [1, 2, 3], 4, 5, 0);
        assert_eq!((o.len(), o[0], &o[4..9]), (32, 0x05, &[4u8, 5, 1, 2, 3][..]));
        let o = output(false, true, [1, 2, 3], 0, 0, 0);
        assert_eq!((o.len(), o[0], &o[8..11]), (78, 0x11, &[1u8, 2, 3][..]));
        assert_eq!(u32::from_le_bytes([o[74], o[75], o[76], o[77]]), bt_crc(&o[..74]));
        let o = output(true, false, [9, 8, 7], 0, 0, 0);
        assert_eq!((o.len(), o[0], o[2], &o[45..48]), (48, 0x02, 0x04, &[9u8, 8, 7][..]));
        let o = output(true, true, [9, 8, 7], 0, 0, 3);
        assert_eq!((o.len(), o[0], o[1], &o[47..50]), (78, 0x31, 0x30, &[9u8, 8, 7][..]));
        // CRC-32 of the empty message after the seed is the CRC-32 of the single byte 0xA2
        assert_eq!(bt_crc(&[]), 0xeada_2d49);
    }

    #[test]
    fn touch_moves_and_scrolls() {
        let mut t = Touch::default();
        assert_eq!(t.step([Some((100, 100)), None]), (0.0, 0.0, 0.0));
        assert_eq!(t.step([Some((110, 95)), None]), (10.0, -5.0, 0.0));
        assert_eq!(t.step([Some((110, 95)), Some((300, 95))]), (0.0, 0.0, 0.0));
        assert_eq!(t.step([Some((110, 105)), Some((300, 105))]), (0.0, 0.0, 10.0));
        assert_eq!(t.step([None, None]), (0.0, 0.0, 0.0));
    }
}
