# librqbit 9.1.0, patched for ReelHouse

Copy of the crates.io release of librqbit 9.1.0 (Apache-2.0, https://github.com/ikatson/rqbit), used through
`[patch.crates-io]` in the top Cargo.toml. The web UI folder is left out (the app does not use that feature).

One change, in `src/piece_tracker.rs` (`acquire_piece`): when a player streams a file, the next few pieces it waits for
could stay with one slow peer for a long time while fast peers fetched later pieces, so playback did not start
(checked on a real torrent: the first bytes of an episode took 13 s with the stock engine and 0.7 s with this change).
Now a head piece that has been in flight longer than 1.5 times the asking peer's average (at least 1.5 s) moves to
that peer.
