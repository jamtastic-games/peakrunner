//! Client-only match connection and discovery API. No server or directory runtime.
mod client;
mod transport;
mod quic;
mod content;
use peakrunner_protocol as proto;
use peakrunner_discovery::{lan as directory, wire};
pub use client::{browse, connect, connect_as, connect_private, connect_with, probe, Download, JoinOptions, Lobby, Probe, Session};
pub use content::Offer;
pub use peakrunner_discovery::{ServerAdvert, PROTOCOL};
pub use peakrunner_protocol::{HOST_KEY_LEN, HOST_READY};
