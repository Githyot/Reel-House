# tokio-wireguard 0.1.3 (vendored)

From https://crates.io/crates/tokio-wireguard (MIT OR Apache-2.0, by raftario), copied here for Reelhouse's IP
protection (src/privacy.rs). Changes:

- boringtun 0.6 -> 0.7.1 (0.6 pins x25519-dalek 2.0.0-rc.3, which cannot be built together with Arti, the Tor client).
- `Peer::preshared_key` (the PresharedKey line of a wg-quick file) is passed to boringtun.
- The examples and dev-dependencies were left out.
