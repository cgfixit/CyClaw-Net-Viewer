//! NetBoard: live TCP/UDP endpoints on Darwin, TCPView-class.

#![cfg_attr(not(target_os = "macos"), allow(unused))]

#[cfg(not(target_os = "macos"))]
compile_error!("NetBoard is macOS-only");

pub mod app;
pub mod cli;
pub mod diff;
pub mod dns;
pub mod kill;
pub mod snapshot;

pub use diff::{diff, Highlight, Row};
pub use snapshot::{
    csv_escape, fmt_addr, snapshot, Dir, Endpoint, EndpointKey, IpVer, Proto, SnapshotError,
    TcpState,
};
