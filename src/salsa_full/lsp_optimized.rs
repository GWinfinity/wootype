//! Optimized LSP implementation with Salsa integration
//!
//! Features:
//! - Incremental document sync
//! - tokio::sync::watch for partial results
//! - Cancellation support
//! - Debounced updates

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, watch, RwLock};
use tokio::time::{interval, Instant};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tracing::{debug, error, info, trace, warn};

use super::{Database, SourceFile, TypeDatabase, completions as salsa_completions, resolve_symbol};
use crate::salsa_full::inputs::{IncrementalChange, ChangeRange};

/// Cancellation token for long-running operations
#[derive(Clone)]
pub struct CancellationToken {
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
    
    pub fn cancel(&self) {
        self.cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::SeqCst)
    }
    
    pub fn check(&self) -> Result<(), tower_lsp::jsonrpc::Error> {
        if self.is_cancelled() {
            Err(tower_lsp::jsonrpc::Error::internal_error())
        } else {
            Ok(())
        }
    }
}

/// LSP server with Salsa database and watch channels
pub struct OptimizedLspServer {
    client: Client,
    db: Arc<RwLock<Database>>,
    /// Watch channel for diagnostics - receivers get notified of changes
    diagnostics_tx: watch::Sender<HashMap<Url, Vec<Diagnostic>>>,
    diagnostics_rx: Arc<watch::Receiver<HashMap<Url, Vec<Diagnostic>>>>,
    /// Open documents with their Salsa file handles
    open_documents: Arc<RwLock<HashMap<Url, DocumentState>>>,
    /// Cancellation tokens for in-flight operations
    cancellations: Arc<RwLock<HashMap<String, CancellationToken>>>,
    /// Debounce timer for changes
    pending_changes: Arc<RwLock<HashMap<Url, Instant>>>,
}

struct DocumentState {
    version: i32,
    file: SourceFile,
    path: PathBuf,
}

impl OptimizedLspServer {
    pub fn new(client: Client) -> Self {
        let (diagnostics_tx, diagnostics_rx) = watch::Sender::new(HashMap::new());
        
        let server = Self {
            client,
            db: Arc::new(RwLock::new(Database::new())),
            diagnostics_tx,
            diagnostics_rx: Arc::new(diagnostics_rx),
            open_documents: Arc::new(RwLock::new(HashMap::new())),
            cancellations: Arc::new(RwLock::new(HashMap::new())),
            pending_changes: Arc::new(RwLock::new(HashMap::new())),
        };
        
        // Start background task to broadcast diagnostics
        server.spawn_diagnostics_broadcaster();
        
        // Start debounce task
        server.spawn_debounce_task();
        
        server
    }
    
    /// Spawn a task to broadcast diagnostics to all clients
    fn spawn_diagnostics_broadcaster(&self) {
        let client = self.client.clone();
        let mut rx = self.diagnostics_rx.clone();
        
        tokio::spawn(async move {
            loop {
                // Wait for changes
                if rx.changed().await.is_err() {
                    break;
                }
                
                let diagnostics = rx.borrow().clone();
                
                // Publish all diagnostics
                for (url, diags) in diagnostics {
                    client.publish_diagnostics(url, diags, None).await;
                }
            }
        });
    }
    
    /// Spawn debounce task for change processing
    fn spawn_debounce_task(&self) {
        let pending = self.pending_changes.clone();
        let db = self.db.clone();
        let diagnostics_tx = self.diagnostics_tx.clone();
        let open_docs = self.open_documents.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(50));
            
            loop {
                interval.tick().await;
                
                let now = Instant::now();
                let ready: Vec<_> = {
                    let pending = pending.read().await;
                    pending.iter()
                        .filter(|(_, time)| now.duration_since(**time) > Duration::from_millis(100))
                        .map(|(url, _)| url.clone())
                        .collect()
                };
                
                for url in ready {
                    pending.write().await.remove(&url);
                    
                    // Run diagnostics
                    if let Some(doc) = open_docs.read().await.get(&url) {
                        let diags = run_diagnostics(&*db.read().await, doc.file).await;
                        let _ = diagnostics_tx.send({
                            let mut map = diagnostics_tx.borrow().clone();
                            map.insert(url, diags);
                            map
                        });
                    }
                }
            }
        });
    }
    
    /// Create a cancellation token for an operation
    async fn create_cancellation_token(&self, id: String) -> CancellationToken {
        let token = CancellationToken::new();
        self.cancellations.write().await.insert(id.clone(), token.clone());
        token
    }
    
    /// Cancel an operation
    async fn cancel_operation(&self, id: &str) {
        if let Some(token) = self.cancellations.write().await.remove(id) {
            token.cancel();
        }
    }
    
    /// Run diagnostics on a file
    async fn run_diagnostics(&self, url: &Url) -> Vec<Diagnostic> {
        let docs = self.open_documents.read().await;
        if let Some(doc) = docs.get(url) {
            let db = self.db.read().await;
            run_diagnostics(&*db, doc.file).await
        } else {
            vec![]
        }
    }
}

async fn run_diagnostics(db: &Database, file: SourceFile) -> Vec<Diagnostic> {
    let result = db.infer_function(file); // Would get functions from file
    
    result.errors(db).iter().map(|e| Diagnostic {
        range: Range {
            start: Position { line: e.span.line as u32, character: e.span.column as u32 },
            end: Position { line: e.span.line as u32, character: (e.span.column + 10) as u32 },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String("E001".to_string())),
        source: Some("wootype".to_string()),
        message: e.message.clone(),
        ..Default::default()
    }).collect()
}

#[tower_lsp::async_trait]
impl LanguageServer for OptimizedLspServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        info!("LSP server initializing with Salsa optimization");
        
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "wootype-salsa".to_string(),
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
                    trigger_characters: Some(vec![".".to_string()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                rename_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }
    
    async fn initialized(&self, _: InitializedParams) {
        self.client.log_message(MessageType::INFO, "Wootype Salsa LSP ready").await;
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
        
        // Create file in Salsa database
        let mut db = self.db.write().await;
        let file = db.create_file(path.clone(), params.text_document.text);
        drop(db);
        
        // Store document state
        self.open_documents.write().await.insert(url.clone(), DocumentState {
            version: params.text_document.version,
            file,
            path,
        });
        
        // Queue diagnostics
        self.pending_changes.write().await.insert(url, Instant::now());
    }
    
    /// Document changed - INCREMENTAL
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let url = params.text_document.uri;
        
        let mut docs = self.open_documents.write().await;
        if let Some(doc) = docs.get_mut(&url) {
            doc.version = params.text_document.version;
            
            let mut db = self.db.write().await;
            
            // Apply incremental changes
            for change in params.content_changes {
                if let Some(range) = change.range {
                    // Incremental update
                    let current = db.source_text(doc.file);
                    let new_content = apply_incremental_change(&current, range, &change.text);
                    db.update_file(doc.file, new_content);
                } else {
                    // Full document update
                    db.update_file(doc.file, change.text);
                }
            }
            
            drop(db);
        }
        drop(docs);
        
        // Queue for debounced diagnostics
        self.pending_changes.write().await.insert(url, Instant::now());
    }
    
    /// Document closed
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let url = params.text_document.uri;
        
        self.open_documents.write().await.remove(&url);
        self.pending_changes.write().await.remove(&url);
        
        // Clear diagnostics
        self.client.publish_diagnostics(url, vec![], None).await;
    }
    
    /// Hover with cancellation support
    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let token = self.create_cancellation_token("hover".to_string()).await;
        
        let url = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        
        // Check cancellation
        token.check()?;
        
        let docs = self.open_documents.read().await;
        if let Some(doc) = docs.get(&url) {
            let db = self.db.read().await;
            let offset = position_to_offset(db.source_text(doc.file), position);
            
            if let Some(symbol) = resolve_symbol(&*db, doc.file, offset) {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("```go\n{} {}\n```", symbol.name, symbol.ty),
                    }),
                    range: None,
                }));
            }
        }
        
        Ok(None)
    }
    
    /// Completion with cancellation
    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let token = self.create_cancellation_token(format!("completion:{}", params.text_document_position.position.line)).await;
        
        let url = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        
        let docs = self.open_documents.read().await;
        if let Some(doc) = docs.get(&url) {
            token.check()?;
            
            let db = self.db.read().await;
            let offset = position_to_offset(db.source_text(doc.file), position);
            
            token.check()?;
            
            let completions = salsa_completions(&*db, doc.file, offset);
            
            let items: Vec<_> = completions.into_iter().map(|c| CompletionItem {
                label: c.label,
                kind: Some(match c.kind {
                    super::CompletionKind::Function => CompletionItemKind::FUNCTION,
                    super::CompletionKind::Variable => CompletionItemKind::VARIABLE,
                    super::CompletionKind::Type => CompletionItemKind::TYPE_PARAMETER,
                    super::CompletionKind::Field => CompletionItemKind::FIELD,
                    super::CompletionKind::Method => CompletionItemKind::METHOD,
                    super::CompletionKind::Module => CompletionItemKind::MODULE,
                    super::CompletionKind::Keyword => CompletionItemKind::KEYWORD,
                }),
                detail: c.detail,
                documentation: c.documentation.map(|d| Documentation::String(d)),
                insert_text: c.insert_text,
                ..Default::default()
            }).collect();
            
            return Ok(Some(CompletionResponse::Array(items)));
        }
        
        Ok(None)
    }
    
    /// Cancellation notification
    async fn cancel(&self, params: CancelParams) {
        if let Some(id) = params.id.as_str() {
            self.cancel_operation(id).await;
        }
    }
}

/// Apply an incremental change to document content
fn apply_incremental_change(content: &str, range: Range, new_text: &str) -> String {
    let start_offset = position_to_offset(content, range.start);
    let end_offset = position_to_offset(content, range.end);
    
    let mut result = content.to_string();
    result.replace_range(start_offset..end_offset, new_text);
    result
}

/// Convert LSP position to byte offset
fn position_to_offset(content: &str, position: Position) -> usize {
    let mut line = 0u32;
    let mut col = 0u32;
    let mut offset = 0usize;
    
    for (i, c) in content.char_indices() {
        if line == position.line && col == position.character {
            return i;
        }
        
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
        offset = i + c.len_utf8();
    }
    
    offset
}

/// Start the optimized LSP server
pub async fn start_lsp_server() -> anyhow::Result<()> {
    info!("Starting Salsa-optimized LSP server");
    
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let (service, socket) = LspService::new(|client| OptimizedLspServer::new(client));
    
    Server::new(stdin, stdout, socket).serve(service).await;
    
    Ok(())
}
