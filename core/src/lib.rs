//! Reelhouse core: everything except the window.
//!
//! The app crate only forwards UI calls to [`engine::Engine`], so all behaviour can be tested here
//! without a display, a browser engine or a real mpv.

pub mod addons;
pub mod anime4k;
pub mod art;
pub mod engine;
pub mod error;
pub mod mpv;
pub mod realdebrid;
pub mod settings;
pub mod streams;
pub mod subtitles;
pub mod translators;

pub use engine::Engine;
pub use error::{Error, Result};
