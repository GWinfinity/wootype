# Phase 3: 语义操作系统 (Semantic OS)

完整替代 gopls 的类型相关功能，支持复杂语义查询。

## 架构设计

```
┌─────────────────────────────────────────────────────────────┐
│                     Semantic OS                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │   Queries    │  │    Checks    │  │   Operations     │  │
│  │  ─────────   │  │  ─────────   │  │   ───────────    │  │
│  │ • Type At    │  │ • Interface  │  │  • Refactor      │  │
│  │ • Def        │  │   Implement  │  │  • Rename        │  │
│  │ • Implement  │  │ • Method Sig │  │  • Extract       │  │
│  │ • References │  │ • Type Cons  │  │  • Inline        │  │
│  │ • Callers    │  │              │  │                  │  │
│  └──────────────┘  └──────────────┘  └──────────────────┘  │
│                          │                                  │
│                    ┌─────┴─────┐                            │
│                    │ Salsa DB  │                            │
│                    │ (Cached)  │                            │
│                    └─────┬─────┘                            │
│                          │                                  │
│                    ┌─────┴─────┐                            │
│                    │ Go Parser │                            │
│                    │  (tree-sitter)                         │
│                    └───────────┘                            │
└─────────────────────────────────────────────────────────────┘
```

## 核心功能

### 1. 复杂语义查询 (Queries)
- `type_at` - 光标位置类型推断
- `definition` - 符号跳转到定义
- `implementation` - 查找接口实现
- `references` - 查找所有引用
- `callers` - 查找调用者
- `callees` - 查找被调用者
- `workspace_symbols` - 工作区符号搜索

### 2. 一致性检查 (Checks)
- `interface_implement_check` - 跨模块接口实现一致性
- `method_signature_check` - 方法签名一致性
- `type_compatibility_check` - 类型兼容性检查
- `import_cycle_check` - 导入循环检测

### 3. 重构操作 (Operations)
- `rename` - 安全重命名
- `extract_function` - 提取函数
- `inline` - 内联展开
- `organize_imports` - 自动导入管理

## 接口实现一致性检查示例

```go
// module_a/types.go
type Reader interface {
    Read(p []byte) (n int, err error)
}

// module_b/impl.go
// 这个实现是否与 module_a.Reader 一致？
type FileReader struct{}
func (f *FileReader) Read(p []byte) (int, error) { ... }
```

检查项：
1. 方法名是否匹配
2. 参数类型是否一致
3. 返回值类型是否一致
4. 接收者类型是否兼容
5. 跨模块时版本是否一致
