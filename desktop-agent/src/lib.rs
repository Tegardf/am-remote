//! am-remote desktop agent: controls Apple Music on Windows and serves the LAN API.
//!
//! The tray lives in the binary (`main.rs`); everything else is here so tests can use it.

pub mod api;
pub mod config;
pub mod discovery;
pub mod media;
pub mod search;
