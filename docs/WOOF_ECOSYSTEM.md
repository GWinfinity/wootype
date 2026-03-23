# Woof 生态系统：woofmt + wootype

## 愿景

构建一个统一的 Go 语言工具生态系统，提供从代码风格到深度语义分析的完整开发者体验。

```
┌─────────────────────────────────────────────────────────────┐
│                      Woof 生态系统                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   ┌──────────────┐        ┌──────────────────┐             │
│   │   woofmt     │        │    wootype       │             │
│   │  ─────────   │        │  ─────────────   │             │
│   │  Linter      │◄──────►│  Type Checker    │             │
│   │  Formatter   │ 共享   │  Semantic OS     │             │
│   │  Code Style  │ 架构   │  Complex Queries │             │
│   └──────────────┘        └──────────────────┘             │
│          │                       │                         │
│          └───────────┬───────────┘                         │
│                      │                                      │
│          ┌───────────▼───────────┐                         │
│          │     Shared Core       │                         │
│          │  ───────────────────  │                         │
│          │  • Salsa 增量计算      │                         │
│          │  • 统一 LSP 服务器     │                         │
│          │  • 符号索引层          │                         │
│          │  • AST 缓存            │                         │
│          │  • 文件系统抽象        │                         │
│          └───────────────────────┘                         │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## 为什么需要生态整合？

### 当前痛点

| 痛点 | 现状 | 解决方案 |
|-----|------|---------|
| **工具割裂** | golint + staticcheck + gopls 各自为政 | 统一架构，共享数据 |
| **重复解析** | 每个工具都解析AST | 统一缓存，增量更新 |
| **配置分散** | 多个配置文件 | 统一配置 (woof.toml) |
| **性能损耗** | 顺序运行多个工具 | 并行分析，结果复用 |
| **IDE 集成复杂** | 多个 LSP 服务器 | 单一 LSP，统一协议 |

### 生态优势

```
用户输入 → woofmt 快速检查（风格、简单错误）
              ↓
         共享 AST 缓存
              ↓
       wootype 深度分析（类型、语义）
              ↓
         统一报告 + 修复建议
```

## 架构整合

### 1. 共享核心层

```rust
// shared-core/src/lib.rs

/// 统一文件管理
pub struct FileManager {
    salsa_db: Database,
    ast_cache: AstCache,
    symbol_index: SymbolIndex,
}

/// 统一诊断报告
pub struct Diagnostic {
    pub code: String,          // "W001" (woofmt) or "T001" (wootype)
    pub severity: Severity,
    pub message: String,
    pub source: ToolSource,    // woofmt | wootype
    pub fix: Option<CodeFix>,
}
```

### 2. 工具链整合

```toml
# woof.toml - 统一配置
[lint]
enable = ["errcheck", "staticcheck", "gosimple"]
disable = ["gocyclo"]

[typecheck]
mode = "gradual"  # static | gradual | dynamic
strict = false

[format]
tab_width = 4
use_tabs = false
max_line_length = 100

[semantic]
enable_cross_module_checks = true
enable_interface_consistency = true
```

### 3. LSP 服务器整合

```rust
// woof-lsp/src/server.rs

pub struct WoofLspServer {
    /// 快速检查（woofmt）
    linter: Arc<LinterEngine>,
    
    /// 深度分析（wootype）
    type_checker: Arc<SemanticOS>,
    
    /// 共享数据库
    db: Database,
}

impl WoofLspServer {
    async fn on_change(&self, params: DidChangeTextDocumentParams) {
        // 1. 快速响应（woofmt）- < 10ms
        let quick_diagnostics = self.linter.check(&params).await;
        self.client.publish_diagnostics(quick_diagnostics).await;
        
        // 2. 深度分析（wootype）- 异步
        tokio::spawn(async move {
            let deep_diagnostics = self.type_checker.analyze(&params).await;
            self.client.publish_diagnostics(deep_diagnostics).await;
        });
    }
}
```

## 功能协作

### 场景 1：保存时自动检查

```go
// user.go
func processUser(u *User) {  // woofmt: 函数体为空警告
    // TODO: implement
}

type User struct {          // wootype: User 未使用
    Name string
}
```

**woofmt 报告**（即时）：
- ⚠️ `processUser` 函数体为空
- 💡 添加 TODO 或实现逻辑

**wootype 报告**（异步）：
- ⚠️ `User` 类型声明但未使用
- 💡 删除或导出使用

### 场景 2：重构协作

```go
// 重构前
func getName() string { return "" }

// 重构后 - woofmt 自动格式化 + wootype 验证类型
func getUserName() string { return "" }
```

**整合流程**：
1. 用户重命名 `getName` → `getUserName`
2. woofmt 格式化新代码（缩进、空格）
3. wootype 验证所有引用点类型正确
4. 统一应用工作区编辑

### 场景 3：导入管理

```go
// woofmt: 检测未使用导入
import (
    "fmt"      // ⚠️ 未使用
    "strings"  // ✅ 使用
)

// wootype: 检测缺失导入
result := strings.TrimSpace(input)  // ✅ OK
fmt.Println(result)                  // ❌ fmt 被 woofmt 移除！
```

**协作解决**：
- woofmt 标记 `fmt` 为未使用
- wootype 检测到 `fmt.Println` 调用
- 协作保留 `fmt` 导入

## 性能提升

### 单独运行 vs 生态整合

| 场景 | 单独运行 | 生态整合 | 提升 |
|-----|---------|---------|------|
| 保存检查 | 50ms + 100ms | 50ms（并行） | **3x** |
| 冷启动 | 12ms + 50ms | 20ms（共享） | **3x** |
| 批量分析 | 150ms + 500ms | 400ms（复用） | **1.6x** |
| 内存占用 | 200MB + 500MB | 400MB（共享） | **1.75x** |

### 共享缓存机制

```
文件修改
    ↓
Salsa 数据库（共享）
    ├─► woofmt 增量检查（语法、风格）
    └─► wootype 增量分析（语义、类型）
            ↓
    统一诊断报告
```

## 实现路线图

### 阶段 1：API 标准化（1-2 周）

- [ ] 定义 `Diagnostic` 统一格式
- [ ] 定义 `WorkspaceEdit` 标准
- [ ] 共享 `Position` / `Range` 类型

### 阶段 2：共享核心（2-4 周）

- [ ] 提取 `shared-core` crate
- [ ] 统一 Salsa 数据库
- [ ] 共享 AST 缓存

### 阶段 3：LSP 整合（3-4 周）

- [ ] 创建 `woof-lsp` 统一服务器
- [ ] 诊断优先级系统
- [ ] 增量更新协议

### 阶段 4：配置统一（1-2 周）

- [ ] 统一 `woof.toml` 配置
- [ ] 配置验证工具
- [ ] 迁移指南

### 阶段 5：生态工具（4+ 周）

- [ ] `woof` CLI 统一入口
- [ ] `woof-ci` CI/CD 集成
- [ ] `woof-action` GitHub Action

## 用户价值

### 对于开发者

```bash
# 之前
gofmt -w .
golint ./...
go vet ./...
staticcheck ./...
gopls check

# 之后
woof check        # 全部完成！
woof fmt          # 格式化
woof lint         # 快速检查
woof type         # 深度分析
woof fix          # 自动修复
```

### 对于 IDE

```json
// 之前 - 需要配置多个 LSP
"go.lintTool": "golangci-lint",
"go.lintFlags": ["--fast"],
"go.toolsManagement.autoUpdate": true

// 之后 - 一个配置搞定
"go.useWoof": true
```

### 对于 CI/CD

```yaml
# 之前
- name: Lint
  run: golangci-lint run --timeout=5m

- name: Type Check
  run: go build ./...

# 之后
- name: Woof Check
  uses: wooorm/woof-action@v1
```

## 开源策略

```
woof-ecosystem/
├── woofmt/          # Linter + Formatter (现有)
├── wootype/         # 类型系统 (现有)
├── woof-core/       # 共享核心 (新建)
├── woof-lsp/        # 统一 LSP (新建)
├── woof-cli/        # 统一 CLI (新建)
└── woof-action/     # GitHub Action (新建)
```

## 总结

woofmt + wootype 生态整合将带来：

1. **性能提升**：共享缓存，减少重复计算
2. **体验统一**：单一配置，一致诊断格式
3. **功能增强**：工具协作，更智能的修复建议
4. **生态扩展**：统一平台，便于社区贡献

**下一步行动**：创建 `woof-core` 共享核心 crate，开始 API 标准化工作。
