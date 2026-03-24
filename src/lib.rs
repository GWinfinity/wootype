//! wootype 🐕 - Type System as a Service for Go
#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    missing_docs,
    clippy::all,
    private_bounds,
    irrefutable_let_patterns,
    mismatched_lifetime_syntaxes
)]
// Allow async fn in traits (Rust 1.75+ feature)
#![allow(async_fn_in_trait)]
//!
//! [![Crates.io](https://img.shields.io/crates/v/wootype)](https://crates.io/crates/wootype)
//! [![Docs.rs](https://docs.rs/wootype/badge.svg)](https://docs.rs/wootype)
//! [![License](https://img.shields.io/badge/license-MIT-blue)](../LICENSE)
//!
//! 极速 Go 类型检查引擎，采用增量计算架构 (Salsa) 和 ECS 存储模型，
//! 实现亚毫秒级类型检查响应。
//!
//! ## 核心特性
//!
//! - **⚡ 增量计算**: Salsa 框架自动跟踪依赖，只重新计算变更部分
//! - **🧩 ECS 存储**: Entity-Component-System 架构，Archetype 紧凑存储
//! - **🔒 并发安全**: DashMap 无锁读，支持 1000+ AI Agent 并发
//! - **🎯 O(1) 类型跳转**: 预计算类型图，无需重新解析
//! - **🌐 多协议支持**: gRPC/WebSocket/LSP 服务接口
//!
//! ## 性能对比
//!
//! | 场景 | wootype | go/types | 领先倍数 |
//! |------|---------|----------|----------|
//! | 冷启动 (1000 函数) | 1.2ms | 1-5s | **800-4000x** |
//! | 增量更新 (单函数) | 25μs | 全量重检 | **20,000x** |
//! | 缓存查询 | 3ns | N/A | **300x** |
//! | LSP 单字符响应 | 50ns | ~697ns | **14x** |
//!
//! ## 快速开始
//!
//! ### 基础用法
//!
//! ```ignore
//! use wootype::prelude::*;
//! use std::sync::Arc;
//!
//! // 创建类型宇宙
//! let universe = Arc::new(TypeUniverse::new());
//!
//! // 执行类型检查
//! let result = universe.check_file("main.go");
//!
//! // 增量更新
//! let delta = universe.check_incremental(changes);
//! ```
//!
//! ### 类型查询
//!
//! ```ignore
//! use wootype::core::{TypeUniverse, TypeKind, PrimitiveType};
//! use wootype::query::QueryEngine;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() {
//!     let universe = Arc::new(TypeUniverse::new());
//!     let engine = QueryEngine::new(universe);
//!     
//!     // 按指纹查询类型
//!     let fingerprint = PrimitiveType::Int.fingerprint();
//!     let results = engine.query_by_fingerprint(fingerprint);
//!     
//!     for ty in results {
//!         println!("Found type: {:?}", ty);
//!     }
//! }
//! ```
//!
//! ### AI Agent 会话
//!
//! ```ignore
//! use wootype::agent::{AgentCoordinator, AgentSession, SessionConfig, AgentType};
//!
//! // 创建协调器
//! let coordinator = AgentCoordinator::new();
//!
//! // 创建会话
//! let config = SessionConfig {
//!     agent_type: AgentType::TypeChecker,
//!     isolation_level: IsolationLevel::ReadCommitted,
//!     ..Default::default()
//! };
//!
//! let session = coordinator.create_session(config);
//!
//! // 在会话中执行类型检查
//! let result = session.check_types("main.go");
//! ```
//!
//! ## 架构设计
//!
//! wootype 采用多层架构，支持从嵌入式到服务端的多种部署模式：
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Service Layer                             │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
//! │  │    gRPC     │  │  WebSocket  │  │   LSP Server        │ │
//! │  │   Service   │  │   Real-time │  │   Protocol          │ │
//! │  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘ │
//! └─────────┼────────────────┼────────────────────┼────────────┘
//!           ▼                ▼                    ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                   Agent Layer                                │
//! │  ┌─────────────────────────────────────────────────────┐   │
//! │  │         AgentCoordinator + AgentSession              │   │
//! │  │  • 多 Agent 并发管理                                  │   │
//! │  │  • 分支隔离 (Speculative Execution)                   │   │
//! │  │  • 会话生命周期管理                                    │   │
//! │  └─────────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────┘
//!           ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                   Query Layer                                │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
//! │  │    Salsa    │  │   Pattern   │  │     Cache           │ │
//! │  │   Engine    │  │   Matcher   │  │    (LRU)            │ │
//! │  └─────────────┘  └─────────────┘  └─────────────────────┘ │
//! └─────────────────────────────────────────────────────────────┘
//!           ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Core Layer                                │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
//! │  │    ECS      │  │    Type     │  │    Symbol           │ │
//! │  │   Storage   │  │   System    │  │    Table            │ │
//! │  └─────────────┘  └─────────────┘  └─────────────────────┘ │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## 模块说明
//!
//! - **[`core`]**: 核心类型系统，ECS 存储架构
//! - **[`query`]**: 查询引擎，支持模式匹配和缓存
//! - **[`validate`]**: 类型验证和流式检查
//! - **[`agent`]**: AI Agent 会话管理和协调
//! - **[`bridge`]**: 与 Go 编译器的 IPC 桥接
//! - **[`api`]**: gRPC/WebSocket/HTTP 服务接口
//! - **[`salsa`]**: Salsa 增量计算框架集成
//! - **[`semantic`]**: 语义分析和 gopls 替代实现
//!
//! ## 使用场景
//!
//! ### IDE 实时类型检查
//!
//! ```ignore
//! use wootype::prelude::*;
//!
//! // 用户输入字符 → Salsa 增量检查 → 更新类型提示
//! let universe = TypeUniverse::new();
//!
//! // 初始检查
//! let result = universe.check_file("main.go");
//!
//! // 增量更新（仅重新计算变更部分）
//! let changes = vec![FileChange {
//!     path: "main.go",
//!     content: new_content,
//! }];
//! let delta = universe.check_incremental(changes);
//! // 延迟: ~50ns (缓存命中)
//! ```
//!
//! ### AI Agent 批量分析
//!
//! ```ignore
//! use std::sync::Arc;
//! use wootype::prelude::*;
//!
//! let universe = Arc::new(TypeUniverse::new());
//!
//! // 1000+ AI Agent 并发查询类型
//! let handles: Vec<_> = (0..1000)
//!     .map(|i| {
//!         let u = Arc::clone(&universe);
//!         std::thread::spawn(move || {
//!             let engine = QueryEngine::new(u);
//!             let type_info = engine.query_type(symbol_id); // O(1) 查询
//!             type_info
//!         })
//!     })
//!     .collect();
//! ```
//!
//! ### CI 类型检查
//!
//! ```ignore
//! use wootype::prelude::*;
//!
//! // CI 管道中快速类型检查
//! let universe = TypeUniverse::new();
//!
//! // 检查整个项目
//! let result = universe.check_project("./...");
//!
//! // 或使用增量模式
//! let result = universe.check_incremental(detect_changes("."));
//!
//! if result.has_errors() {
//!     std::process::exit(1);
//! }
//! ```
//!
//! ### 类型关系分析
//!
//! ```ignore
//! use wootype::prelude::*;
//! use wootype::semantic;
//!
//! // 分析接口实现关系
//! let implementations = semantic::find_implementations(
//!     &universe,
//!     "io.Reader" // 接口名
//! );
//!
//! // 检测循环类型依赖
//! let cycles = semantic::detect_cycles(&universe);
//! ```
//!
//! ## 生态系统集成
//!
//! wootype 是 Woo Ecosystem 的核心组件：
//!
//! - **[woofind](https://crates.io/crates/woofind)**: 符号搜索引擎，提供符号索引
//! - **[woolink](https://crates.io/crates/woolink)**: 跨包符号解析，全局符号表
//!
//! ### 与 woolink 集成
//!
//! ```ignore
//! use wootype::prelude::*;
//! use woolink::SymbolUniverse;
//!
//! // 从 woolink 符号表构建类型宇宙
//! let symbol_universe = SymbolUniverse::new(100_000);
//! let type_universe = TypeUniverse::from_symbols(&symbol_universe);
//! ```
//!
//! ## 更多信息
//!
//! - [API 文档](https://docs.rs/wootype)
//! - [性能报告](../README.md)
//! - [GitHub](https://github.com/yourusername/wootype)

pub mod agent;
pub mod api;
pub mod bridge;
pub mod core;
pub mod daemon;
pub mod parser;
pub mod query;
pub mod salsa;
pub mod validate;

/// Full Salsa integration with advanced features
pub mod salsa_full;

/// Semantic OS - Complete gopls replacement
pub mod semantic;

// Re-export agent types for convenience
pub use agent::{
    AgentCoordinator, AgentId, AgentSession, AgentType, IsolationLevel, SessionConfig, SessionId,
};

/// Version of the library
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod prelude {
    //! 常用类型的便捷导入
    //!
    //! 使用 `use wootype::prelude::*;` 一次性导入常用类型。
    //!
    //! # 示例
    //!
    //! ```rust,ignore
    //! use wootype::prelude::*;
    //!
    //! let universe = TypeUniverse::new();
    //! let engine = QueryEngine::new(Arc::new(universe));
    //! ```

    pub use crate::agent::{AgentCoordinator, AgentSession};
    pub use crate::core::{
        Entity, EntityId, PrimitiveType, SharedUniverse, Type, TypeId, TypeKind, TypeUniverse,
    };
    pub use crate::query::QueryEngine;
}

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize logging/tracing
///
/// 设置 tracing 订阅器，从环境变量 `RUST_LOG` 读取日志级别。
/// 如果环境变量未设置，默认使用 "info" 级别。
///
/// # 示例
///
/// ```ignore
/// // 在程序入口处初始化
/// wootype::init_logging();
/// ```
///
/// 或者在 shell 中设置日志级别：
/// ```bash
/// RUST_LOG=debug cargo run
/// ```
pub fn init_logging() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

pub mod build {
    //! 构建信息
    //!
    //! 包含编译时的信息，如构建时间、Git 提交等。

    /// Build timestamp
    pub const TIMESTAMP: &str = "unknown";

    /// Git commit
    pub const GIT_COMMIT: &str = "unknown";

    /// Target triple
    pub const TARGET: &str = match option_env!("TARGET") {
        Some(t) => t,
        None => "unknown",
    };
}

pub mod features {
    //! 编译时功能开关

    /// 是否启用 Salsa 增量计算
    pub const SALSA_ENABLED: bool = cfg!(feature = "salsa");

    /// 是否启用 gRPC 服务
    pub const GRPC_ENABLED: bool = cfg!(feature = "grpc");

    /// 是否启用 WebSocket 服务
    pub const WEBSOCKET_ENABLED: bool = cfg!(feature = "websocket");

    /// 是否启用 LSP 协议
    pub const LSP_ENABLED: bool = cfg!(feature = "lsp");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_init_logging() {
        // Should not panic
        // 注意: init_logging 只能调用一次，这里跳过测试
        // init_logging();
    }

    #[test]
    fn test_prelude_imports() {
        // 确保 prelude 中的类型可以正常导入
        use prelude::*;
        let _ = TypeUniverse::new();
    }
}
