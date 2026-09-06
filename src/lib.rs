//! CyClaw-Net-Viewer library (`netboard`): live TCP/UDP endpoints on Darwin.

#![cfg_attr(not(target_os = "macos"), allow(unused))]

#[cfg(not(target_os = "macos"))]
compile_error!("CyClaw-Net-Viewer is macOS-only");

pub mod app;
pub mod cli;
pub mod diff;
pub mod dns;
pub mod export;
pub mod kill;
pub mod snapshot;

pub use diff::{diff, Highlight, Row};
pub use snapshot::{
    csv_escape, fmt_addr, is_offbox, snapshot, Dir, Endpoint, EndpointKey, IpVer, Proto,
    SnapshotError, TcpState,
};
