# Wootype - Type System as a Service for Go

A Rust-powered type checker providing zero-latency type queries for AI coding assistants.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     AI Agents (Cursor, Claude, etc.)            │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  gRPC API / WebSocket    │    HTTP REST    │    IPC Bridge      │
│  (Streaming Validation)  │    (Query)      │    (Go Compiler)   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                        Agent Coordinator                        │
│              (Multi-Agent Session Management)                   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                         Type Universe                           │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │ ECS Storage  │  │ Query Engine │  │ Streaming Validator  │  │
│  │ (Archetype)  │  │ (SIMD)       │  │ (Token-by-Token)     │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Go Compiler (via IPC)                        │
│              (Final Build & Code Generation)                    │
└─────────────────────────────────────────────────────────────────┘
```

## Phase 1: 共存架构 (Hybrid Mode) ✅

### Completed Components

#### 1. Core Type System (`src/core/`)
- **ECS-based storage**: Archetype pattern for cache-friendly type storage
- **TypeUniverse**: Central type registry with lock-free indexing
- **Symbol table**: String interning for efficient identifier management
- **Primitive types**: All Go primitive types with fingerprinting
- **Speculative transactions**: Copy-on-write for AI code generation

#### 2. Query Engine (`src/query/`)
- **Zero-latency queries**: O(1) type lookup by ID
- **SIMD-ready fingerprints**: Fast type similarity comparison
- **Interface satisfaction**: Check type implements interface
- **Pattern matching**: Declarative type queries
- **LRU cache**: Sub-microsecond repeated queries

#### 3. Streaming Validation (`src/validate/`)
- **Token-by-token validation**: Real-time feedback for AI generation
- **Look-ahead inference**: Predict next likely types
- **Soft errors**: Non-blocking type guidance
- **Expression-level granularity**: Fine-grained validation

#### 4. Multi-Agent Concurrency (`src/agent/`)
- **Session isolation**: Each AI agent gets isolated branch
- **Copy-on-write**: Efficient memory sharing
- **Branch management**: Commit/rollback support
- **RAG embeddings**: Semantic type search

#### 5. IPC Bridge (`src/bridge/`)
- **Unix socket IPC**: Fast local communication
- **Protocol messages**: Structured request/response
- **gopls shim**: Integration with Go language server
- **Go compiler proxy**: Build integration

#### 6. gRPC API (`src/api/`)
- **Type service**: Query and validate types
- **Streaming validation**: Real-time token validation
- **Session management**: Connect/disconnect agents

#### 7. Parser (`src/parser/`)
- **AST representation**: Go source code AST
- **Package importer**: Import Go packages
- **Type converter**: AST to TypeUniverse

### Usage

```bash
# Start the daemon
wootype daemon --preload-stdlib

# Connect as an AI Agent
wootype connect --name "Cursor" --agent-type cursor

# Import a package
wootype import github.com/example/mypackage

# Query types
wootype query --type int --package main

# Validate expression
wootype validate --session <id> --expr "x + 1"
```

### API Example

```rust
use wootype::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // Create type universe
    let universe = Arc::new(TypeUniverse::new());
    
    // Create query engine
    let engine = QueryEngine::new(universe.clone());
    
    // Query type by fingerprint
    let results = engine.query_by_fingerprint(fingerprint);
    
    // Check interface implementation
    let implements = engine.implements_interface(concrete_id, interface_id);
}
```

## Key Technologies

- **Rust**: Memory-safe systems programming
- **ECS (Entity Component System)**: Cache-friendly data layout
- **DashMap**: Lock-free concurrent hashmap
- **im**: Immutable data structures for branches
- **Tokio**: Async runtime
- **Tonic**: gRPC framework
- **Rayon**: Data parallelism

## Performance Targets

| Operation | Target Latency |
|-----------|---------------|
| Type lookup by ID | < 100ns |
| Fingerprint query | < 1μs |
| Interface check (cached) | < 500ns |
| Streaming validation | < 1ms/token |
| Type similarity search | < 10ms |

## Roadmap

### Phase 1: Hybrid Mode ✅
- [x] Incremental type checking engine
- [x] IPC bridge to Go compiler
- [x] Basic gRPC API

### Phase 2: Servicefication ✅
- [x] gRPC API (TypeDaemon service)
- [x] WebSocket API
- [x] Standalone daemon (`wootype-daemon`)
- [x] Multi-protocol support (gRPC + WebSocket + IPC)
- [x] Client SDK and examples

See [PHASE2_SERVICIFICATION.md](docs/PHASE2_SERVICIFICATION.md) for details.

### Phase 3: Semantic OS (Future)
- [ ] Complex cross-module queries
- [ ] AI-optimized type inference
- [ ] Vector-based semantic search
- [ ] Type system DSL

## License

MIT License - See LICENSE file for details

## Contributing

Contributions welcome! Please see CONTRIBUTING.md for guidelines.
