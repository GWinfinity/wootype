# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2024-03-24

### ✨ Added

#### 1. Go Modules 完整支持
- 完整的 Go Modules 解析 (`src/core/gomod_resolver.rs`)
- 标准库自动识别（`fmt`, `net/http`, `context` 等）
- go.mod 解析（require/replace/exclude/retract 指令）
- 工作区模式支持（go.work）
- 依赖替换和代理支持

#### 2. 跨包代码导航
- 全局符号索引 (`src/core/xpackage.rs`)
- 跨包定义跳转（O(1) 查询）
- 包依赖图分析
- 循环依赖检测
- `CrossPackageNavigator` 导航器

#### 3. 增量更新优化
- 细粒度依赖追踪 (`src/salsa_full/incremental.rs`)
- `DependencyGraph` 依赖图
- 并行增量处理（Rayon）
- 智能影响范围计算
- `IncrementalProcessor` 处理器

### 📝 Documentation
- 新增功能文档 `docs/FEATURES_3_ENHANCEMENTS.md`
- 性能测试报告 `PERFORMANCE_REPORT.md`
- 使用示例 `examples/modules_navigation_incremental.rs`
- 性能测试 `examples/performance_test.rs`

### ✅ Testing
- 10 个新功能集成测试
- 性能基准测试
- 全部 169 个测试通过

### 🚀 Performance
- 类型查询: **70 ns**
- 增量更新加速: **457x**
- 跨包导航: **118 ns**
- 符号处理: **0.92M ops/s**

## [0.1.0] - 2024-03-XX

### Added
- 初始版本发布
- ECS 存储架构
- Salsa 增量计算框架
- 基础类型检查
- LSP 协议支持
- gRPC/WebSocket API
