//! Go compiler shim
//!
//! Provides compatibility layer for gopls and Go compiler integration.

use super::protocol::{Request, Response, SourcePosition, TypeCheckContext};
use crate::agent::AgentSession;
use crate::core::{SharedUniverse, TypeId};

use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tracing::{error, info};

/// Shim for Go compiler integration
pub struct GoCompilerShim {
    universe: SharedUniverse,
    gopls_path: Option<String>,
    go_path: String,
}

impl GoCompilerShim {
    pub fn new(universe: SharedUniverse) -> Self {
        Self {
            universe,
            gopls_path: None,
            go_path: "go".to_string(),
        }
    }

    pub fn with_gopls(mut self, path: impl Into<String>) -> Self {
        self.gopls_path = Some(path.into());
        self
    }

    pub fn with_go(mut self, path: impl Into<String>) -> Self {
        self.go_path = path.into();
        self
    }

    /// Start gopls in stdio mode and bridge to wootype
    pub async fn start_gopls_bridge(&self) -> Result<GoplsBridge, ShimError> {
        let gopls_path = self
            .gopls_path
            .clone()
            .unwrap_or_else(|| "gopls".to_string());

        info!("Starting gopls bridge: {}", gopls_path);

        let mut child = Command::new(&gopls_path)
            .arg("-rpc.trace")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ShimError::Spawn(e))?;

        let stdin = child.stdin.take().ok_or(ShimError::NoStdin)?;
        let stdout = child.stdout.take().ok_or(ShimError::NoStdout)?;

        Ok(GoplsBridge {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    /// Run go build with wootype type checking
    pub async fn go_build(&self, package: &str) -> Result<BuildResult, ShimError> {
        info!("Running go build for: {}", package);

        let output = Command::new(&self.go_path)
            .args(&["build", "-v", package])
            .output()
            .await
            .map_err(|e| ShimError::Io(e))?;

        let success = output.status.success();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(BuildResult {
            success,
            stdout,
            stderr,
            type_errors: vec![],
        })
    }

    /// Sync types with Go compiler
    pub async fn sync_types(&self, session: &AgentSession) -> Result<SyncResult, ShimError> {
        // Export types from session to Go format
        // This would serialize types in a format Go can understand

        Ok(SyncResult {
            types_exported: 0,
            types_imported: 0,
        })
    }
}

/// gopls bridge handle
pub struct GoplsBridge {
    child: Child,
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
}

impl GoplsBridge {
    /// Read a message from gopls
    pub async fn read_message(&mut self) -> Result<Option<String>, ShimError> {
        let mut line = String::new();
        match self.stdout.read_line(&mut line).await {
            Ok(0) => Ok(None),
            Ok(_) => Ok(Some(line)),
            Err(e) => Err(ShimError::Io(e)),
        }
    }

    /// Send a message to gopls
    pub async fn send_message(&mut self, message: &str) -> Result<(), ShimError> {
        self.stdin
            .write_all(message.as_bytes())
            .await
            .map_err(|e| ShimError::Io(e))?;
        self.stdin
            .write_all(b"\n")
            .await
            .map_err(|e| ShimError::Io(e))?;
        Ok(())
    }

    /// Forward LSP request to wootype for type checking
    pub async fn handle_lsp_request(&self, _request: &str) -> Result<String, ShimError> {
        // Parse LSP request, potentially use wootype for type operations
        // Then forward to gopls or respond directly

        Ok(r#"{"jsonrpc":"2.0","id":1,"result":null}"#.to_string())
    }
}

/// Build result
#[derive(Debug, Clone)]
pub struct BuildResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub type_errors: Vec<TypeError>,
}

/// Type error from build
#[derive(Debug, Clone)]
pub struct TypeError {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
}

/// Sync result
#[derive(Debug, Clone)]
pub struct SyncResult {
    pub types_exported: usize,
    pub types_imported: usize,
}

/// Shim error
#[derive(Debug)]
pub enum ShimError {
    Spawn(std::io::Error),
    Io(std::io::Error),
    NoStdin,
    NoStdout,
    Protocol(String),
}

impl std::fmt::Display for ShimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "Failed to spawn: {}", e),
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::NoStdin => write!(f, "No stdin available"),
            Self::NoStdout => write!(f, "No stdout available"),
            Self::Protocol(s) => write!(f, "Protocol error: {}", s),
        }
    }
}

impl std::error::Error for ShimError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TypeUniverse;

    #[tokio::test]
    async fn test_shim_creation() {
        let universe = Arc::new(TypeUniverse::new());
        let shim = GoCompilerShim::new(universe);

        // Just verify it doesn't panic
        assert_eq!(shim.go_path, "go");
    }
}
