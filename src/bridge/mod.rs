//! IPC Bridge for Go compiler integration
//!
//! Connects wootype with the Go compiler via IPC/shared memory.

pub mod ipc;
pub mod protocol;
pub mod shim;

pub use ipc::{BridgeConfig, IpcBridge};
pub use protocol::{Message, Request, Response, SourcePosition, TypeCheckContext};
pub use shim::{BuildResult, GoCompilerShim};
