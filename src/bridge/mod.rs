//! IPC Bridge for Go compiler integration
//! 
//! Connects wootype with the Go compiler via IPC/shared memory.

pub mod ipc;
pub mod protocol;
pub mod shim;

pub use ipc::{IpcBridge, BridgeConfig};
pub use protocol::{Message, Request, Response, TypeCheckContext, SourcePosition};
pub use shim::{GoCompilerShim, BuildResult};
