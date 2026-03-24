//! LSP (Language Server Protocol) integration with Salsa
//!
//! Optimized for real-time editor feedback using incremental computation.

use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tracing::{debug, info, warn};

use crate::salsa::{ChangeRange, IncrementalChange, IncrementalDb, InputManager};
use std::path::PathBuf;

/// LSP server state
pub struct LspServer {
    client: Client,
    db: Arc<RwLock<IncrementalDb>>,
    inputs: Arc<InputManager>,
    open_documents: Arc<dashmap::DashMap<Url, DocumentState>>,
}

#[derive(Clone, Debug)]
struct DocumentState {
    version: i32,
    path: PathBuf,
}

impl LspServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            db: Arc::new(RwLock::new(IncrementalDb::new())),
            inputs: Arc::new(InputManager::new()),
            open_documents: Arc::new(dashmap::DashMap::new()),
        }
    }

    /// Run diagnostics on a file
    async fn run_diagnostics(&self, url: &Url) {
        let path = url.to_file_path().ok();
        if path.is_none() {
            return;
        }
        let path = path.unwrap();

        debug!("Running diagnostics for {:?}", path);

        // Get the file content
        let content = self.inputs.get_file(&path);
        if content.is_none() {
            return;
        }
        let content = content.unwrap();

        // Get database read lock
        let db = self.db.read().await;

        // Find the source file
        // In a real implementation, we'd look up the file in the database
        // and run queries on it

        // For now, create diagnostics based on simple checks
        let diagnostics = self.check_content(&content, &path);

        drop(db);

        // Publish diagnostics
        self.client
            .publish_diagnostics(url.clone(), diagnostics, None)
            .await;
    }

    /// Quick content check for demonstration
    fn check_content(&self, content: &str, _path: &PathBuf) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Simple checks
        for (line_num, line) in content.lines().enumerate() {
            // Check for unused imports (simplified)
            if line.contains("import") && !line.contains("_") {
                // In a real implementation, we'd check if the import is used
            }

            // Check for common issues
            if line.contains("== nil") && line.contains("&&") {
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: Position {
                            line: line_num as u32,
                            character: 0,
                        },
                        end: Position {
                            line: line_num as u32,
                            character: line.len() as u32,
                        },
                    },
                    severity: Some(DiagnosticSeverity::WARNING),
                    code: Some(NumberOrString::String("E001".to_string())),
                    source: Some("wootype".to_string()),
                    message: "Potential nil pointer dereference".to_string(),
                    ..Default::default()
                });
            }
        }

        diagnostics
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for LspServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        info!("LSP server initializing");

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "wootype".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        will_save: Some(false),
                        will_save_wait_until: Some(false),
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(false),
                        })),
                    },
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        info!("LSP server initialized");
        self.client
            .log_message(MessageType::INFO, "Wootype LSP server ready")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        info!("LSP server shutting down");
        Ok(())
    }

    /// Document opened
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let url = params.text_document.uri;
        let path = match url.to_file_path() {
            Ok(p) => p,
            Err(_) => return,
        };

        info!("Document opened: {:?}", path);

        // Register the file
        self.inputs
            .set_file(path.clone(), params.text_document.text);
        self.open_documents.insert(
            url.clone(),
            DocumentState {
                version: params.text_document.version,
                path: path.clone(),
            },
        );

        // Run initial diagnostics
        self.run_diagnostics(&url).await;
    }

    /// Document changed (INCREMENTAL)
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let url = params.text_document.uri;
        let path = match url.to_file_path() {
            Ok(p) => p,
            Err(_) => return,
        };

        debug!(
            "Document changed: {:?} ({} changes)",
            path,
            params.content_changes.len()
        );

        // Apply incremental changes
        for change in params.content_changes {
            if let Some(range) = change.range {
                // Incremental change
                let incremental_change = IncrementalChange {
                    file: path.clone(),
                    range: ChangeRange {
                        start_line: range.start.line as usize,
                        start_col: range.start.character as usize,
                        end_line: range.end.line as usize,
                        end_col: range.end.character as usize,
                    },
                    new_text: change.text,
                };

                if let Err(e) = self.inputs.apply_change(incremental_change) {
                    warn!("Failed to apply change: {}", e);
                }
            } else {
                // Full document sync
                self.inputs.set_file(path.clone(), change.text);
            }
        }

        // Update document state
        if let Some(mut state) = self.open_documents.get_mut(&url) {
            state.version = params.text_document.version;
        }

        // Run diagnostics after a short debounce
        // In production, you'd use a timer to debounce rapid changes
        self.run_diagnostics(&url).await;
    }

    /// Document closed
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let url = params.text_document.uri;

        if let Some((_, state)) = self.open_documents.remove(&url) {
            info!("Document closed: {:?}", state.path);

            // Clear diagnostics
            self.client.publish_diagnostics(url, vec![], None).await;
        }
    }

    /// Document saved
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let url = params.text_document.uri;
        info!("Document saved: {}", url);

        // Run full diagnostics on save
        self.run_diagnostics(&url).await;
    }

    /// Completion request
    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let url = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        debug!(
            "Completion request at {}:{}",
            position.line, position.character
        );

        // In a real implementation, we'd query the database for completions
        let items = vec![
            CompletionItem {
                label: "int".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Built-in integer type".to_string()),
                ..Default::default()
            },
            CompletionItem {
                label: "string".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Built-in string type".to_string()),
                ..Default::default()
            },
            CompletionItem {
                label: "bool".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Built-in boolean type".to_string()),
                ..Default::default()
            },
        ];

        Ok(Some(CompletionResponse::Array(items)))
    }

    /// Hover information
    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let position = params.text_document_position_params.position;

        debug!("Hover request at {}:{}", position.line, position.character);

        // In a real implementation, we'd look up the type at the position
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: "```go\ntype int\n```\n\nBuilt-in integer type".to_string(),
            }),
            range: None,
        }))
    }

    /// Go to definition
    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let position = params.text_document_position_params.position;

        debug!(
            "Definition request at {}:{}",
            position.line, position.character
        );

        // In a real implementation, we'd resolve the definition
        Ok(None)
    }
}

/// Start the LSP server
pub async fn start_lsp_server() -> anyhow::Result<()> {
    info!("Starting LSP server");

    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());

    let (service, socket) = LspService::new(|client| LspServer::new(client));

    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}

/// Start LSP server over TCP (for remote development)
pub async fn start_lsp_server_tcp(addr: &str) -> anyhow::Result<()> {
    info!("Starting LSP server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    loop {
        let (stream, addr) = listener.accept().await?;
        info!("LSP client connected: {}", addr);

        let (read, write) = tokio::io::split(stream);
        let (service, socket) = LspService::new(|client| LspServer::new(client));

        tokio::spawn(async move {
            Server::new(read, write, socket).serve(service).await;
        });
    }
}
