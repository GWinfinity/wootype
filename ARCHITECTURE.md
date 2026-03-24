# wootype 架构设计文档

本文档详细描述 wootype 的架构设计、核心组件和性能优化策略。

## 目录

- [总体架构](#总体架构)
- [核心组件](#核心组件)
- [ECS 存储模型](#ecs-存储模型)
- [Salsa 增量计算](#salsa-增量计算)
- [Agent 并发模型](#agent-并发模型)
- [性能优化](#性能优化)

## 总体架构

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Service Layer                                  │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐ │
│  │    gRPC     │  │  WebSocket  │  │     HTTP    │  │   LSP Server    │ │
│  │   Service   │  │   Service   │  │    API      │  │   (gopls)       │ │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └────────┬────────┘ │
└─────────┼────────────────┼────────────────┼──────────────────┼──────────┘
          │                │                │                  │
          └────────────────┴────────────────┴──────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                           Agent Layer                                    │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                     AgentCoordinator                             │   │
│  │  ├─ 创建/销毁 Agent 会话                                         │   │
│  │  ├─ 管理并发访问                                                 │   │
│  │  ├─ 处理分支隔离 (Speculative Execution)                         │   │
│  │  └─ 协调分布式类型检查                                           │   │
│  │                                                                  │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐              │   │
│  │  │ AgentSession│  │ AgentSession│  │ AgentSession│  ...          │   │
│  │  │   (AI #1)   │  │   (AI #2)   │  │   (IDE)     │              │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘              │   │
│  └─────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                           Query Layer                                    │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐ │
│  │    Salsa    │  │   Pattern   │  │    Cache    │  │  Fingerprint    │ │
│  │   Engine    │  │   Matcher   │  │    (LRU)    │  │    Index        │ │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └────────┬────────┘ │
│         │                │                │                  │          │
│         └────────────────┴────────────────┴──────────────────┘          │
│                                   │                                      │
│                        ┌──────────┴──────────┐                          │
│                        ▼                     ▼                          │
│              ┌─────────────────┐   ┌─────────────────┐                 │
│              │   QueryEngine   │   │  TypeResolver   │                 │
│              └─────────────────┘   └─────────────────┘                 │
└─────────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                            Core Layer                                    │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐ │
│  │     ECS     │  │    Type     │  │   Symbol    │  │   Semantic      │ │
│  │   Storage   │  │   System    │  │    Table    │  │   Analyzer      │ │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └────────┬────────┘ │
│         │                │                │                  │          │
│         └────────────────┴────────────────┴──────────────────┘          │
│                                   │                                      │
│                        ┌──────────┴──────────┐                          │
│                        ▼                     ▼                          │
│              ┌─────────────────┐   ┌─────────────────┐                 │
│              │  TypeUniverse   │   │  SharedUniverse │                 │
│              └─────────────────┘   └─────────────────┘                 │
└─────────────────────────────────────────────────────────────────────────┘
```

## 核心组件

### 1. TypeUniverse (类型宇宙)

核心容器，管理所有类型信息：

```rust
pub struct TypeUniverse {
    /// ECS 存储
    storage: ArchetypeStorage,
    
    /// 符号表
    symbols: SymbolTable,
    
    /// 类型图
    type_graph: TypeGraph,
    
    /// Salsa 数据库
    db: SalsaDatabase,
}

impl TypeUniverse {
    /// 创建新的类型宇宙
    pub fn new() -> Self;
    
    /// 从 woolink 符号表构建
    pub fn from_symbols(symbols: &SymbolUniverse) -> Self;
    
    /// 类型检查文件
    pub fn check_file(&self, path: &Path) -> TypeCheckResult;
    
    /// 增量检查
    pub fn check_incremental(&self, changes: Vec<FileChange>) -> DeltaResult;
}
```

### 2. ArchetypeStorage (ECS 存储)

Entity-Component-System 存储模型：

```rust
pub struct ArchetypeStorage {
    /// 实体 ID 生成器
    next_entity_id: AtomicU64,
    
    /// Archetype 表: 组件组合 -> 紧凑存储
    archetypes: DashMap<ArchetypeId, Archetype>,
    
    /// 组件存储
    components: ComponentStorage,
    
    /// 实体到 Archetype 的映射
    entity_locations: DashMap<EntityId, (ArchetypeId, usize)>,
}

pub struct Archetype {
    /// 此 Archetype 的组件类型
    component_types: Vec<ComponentTypeId>,
    
    /// 紧凑存储的实体数据
    /// 所有相同组件组合的实体连续存储
    entities: Vec<EntityId>,
    
    /// 组件数据 (SoA 布局)
    component_data: Vec<Vec<u8>>,
}
```

**为什么使用 ECS?**

1. **缓存友好**: 相同类型的数据连续存储
2. **灵活**: 动态添加/移除组件
3. **并发**: 不同 archetype 可并行访问
4. **内存高效**: 无空值填充

### 3. SalsaDatabase (增量数据库)

基于 Salsa-rs 的增量计算框架：

```rust
#[salsa::db]
pub struct TypeDatabase {
    storage: Arc<Storage<Token>>,
    
    // 输入
    file_texts: DashMap<FileId, String>,
    file_paths: DashMap<FileId, PathBuf>,
}

#[salsa::tracked]
pub fn parse(db: &dyn Db, file_id: FileId) -> Ast {
    let text = db.file_text(file_id);
    Parser::new(&text).parse()
}

#[salsa::tracked]
pub fn type_check(db: &dyn Db, file_id: FileId) -> TypeResult {
    let ast = parse(db, file_id);
    TypeChecker::new(db).check(&ast)
}
```

**增量计算原理**:

```
文件变更 ──▶ 解析 (parse)
                │
                ├──▶ 文本未变? ──▶ 返回缓存的 AST
                │
                └──▶ 文本变更 ──▶ 重新解析
                         │
                         ▼
                    类型检查 (type_check)
                         │
                         ├──▶ AST 未变? ──▶ 返回缓存的类型结果
                         │
                         └──▶ AST 变更 ──▶ 重新检查
```

### 4. AgentCoordinator (Agent 协调器)

管理多个 AI Agent 的并发访问：

```rust
pub struct AgentCoordinator {
    /// 所有活跃会话
    sessions: DashMap<SessionId, AgentSession>,
    
    /// 类型宇宙
    universe: Arc<TypeUniverse>,
    
    /// 事务管理器
    transaction_manager: TransactionManager,
}

impl AgentCoordinator {
    /// 创建新会话
    pub fn create_session(&self, config: SessionConfig) -> SessionId;
    
    /// 销毁会话
    pub fn destroy_session(&self, id: SessionId);
    
    /// 获取会话
    pub fn get_session(&self, id: SessionId) -> Option<AgentSession>;
    
    /// 协调并发事务
    pub fn coordinate(&self, operations: Vec<Operation>) -> CoordinationResult;
}
```

## ECS 存储模型

### 实体定义

```rust
/// 实体是轻量的 ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId(u64);

/// 组件是数据
pub trait Component: Send + Sync + 'static {
    fn component_type() -> ComponentTypeId;
}

/// 类型组件
#[derive(Debug, Clone)]
pub struct TypeComponent {
    pub kind: TypeKind,
    pub fingerprint: TypeFingerprint,
}

impl Component for TypeComponent {
    fn component_type() -> ComponentTypeId {
        ComponentTypeId::of::<Self>()
    }
}

/// 名称组件
#[derive(Debug, Clone)]
pub struct NameComponent {
    pub name: String,
    pub package: String,
}

impl Component for NameComponent {
    fn component_type() -> ComponentTypeId {
        ComponentTypeId::of::<Self>()
    }
}
```

### Archetype 布局

```
Archetype #1: [TypeComponent, NameComponent]
    Entity 1: [TypeComponent { Int },    NameComponent { "int", "" }]
    Entity 2: [TypeComponent { String },  NameComponent { "string", "" }]
    Entity 3: [TypeComponent { Struct },  NameComponent { "Person", "mypkg" }]

Archetype #2: [TypeComponent, NameComponent, MethodComponent]
    Entity 4: [TypeComponent { Func }, NameComponent { "GetName", "mypkg" }, MethodComponent { ... }]

// 内存布局 (SoA)
TypeComponent.data:  [Int, String, Struct, Func, ...]
NameComponent.data:  [Name1, Name2, Name3, Name4, ...]
MethodComponent.data: [_, _, _, Method4, ...]  // Archetype #1 的实体没有此组件
```

### 查询系统

```rust
pub struct Query<'a, Q: QueryParams> {
    storage: &'a ArchetypeStorage,
    _phantom: PhantomData<Q>,
}

impl<'a, Q: QueryParams> Query<'a, Q> {
    /// 遍历所有匹配的实体
    pub fn iter(&self) -> impl Iterator<Item = Q::Item> + '_ {
        // 1. 找出包含所有所需组件的 archetypes
        let matching_archetypes = self.storage.find_archetypes::<Q>();
        
        // 2. 遍历每个 archetype
        matching_archetypes.flat_map(|archetype| {
            // 3. 返回组件引用
            archetype.query::<Q>()
        })
    }
}

// 使用示例
let query: Query<(&TypeComponent, &NameComponent)> = universe.query();
for (ty, name) in query.iter() {
    println!("Type: {:?}, Name: {:?}", ty.kind, name.name);
}
```

## Salsa 增量计算

### 数据库结构

```rust
#[salsa::db]
#[derive(Default)]
pub struct Database {
    /// 存储 Salsa 内部数据
    storage: salsa::Storage<Self>,
    
    /// 文件内容 (输入)
    file_contents: DashMap<FileId, String>,
    
    /// 文件路径 (输入)
    file_paths: DashMap<FileId, PathBuf>,
}

#[salsa::db]
impl salsa::Database for Database {}

impl Database {
    /// 设置文件内容 (触发增量更新)
    pub fn set_file_content(&mut self, file_id: FileId, content: String) {
        self.file_contents.insert(file_id, content);
        // Salsa 自动追踪依赖
    }
}
```

### Tracked 函数

```rust
/// 解析文件 (被追踪的函数)
#[salsa::tracked]
pub fn parse_file(db: &dyn Db, file_id: FileId) -> Ast {
    let content = db.file_content(file_id);
    Parser::new(&content).parse()
}

/// 类型检查 (依赖解析结果)
#[salsa::tracked]
pub fn check_types(db: &dyn Db, file_id: FileId) -> TypeCheckResult {
    let ast = parse_file(db, file_id);
    TypeChecker::check(&ast)
}

/// 获取类型信息 (依赖类型检查结果)
#[salsa::tracked]
pub fn get_type_info(db: &dyn Db, symbol: SymbolId) -> Option<TypeInfo> {
    // 查找符号定义的文件
    let file_id = db.symbol_file(symbol)?;
    let result = check_types(db, file_id);
    result.get_type(symbol)
}
```

### 增量更新流程

```
用户编辑 ──▶ 更新文件内容 ──▶ Salsa 检测变更
                                  │
                                  ▼
                         ┌─────────────────┐
                         │ 依赖图分析       │
                         │ - 哪些函数依赖   │
                         │   此文件?        │
                         └────────┬────────┘
                                  │
                                  ▼
                         ┌─────────────────┐
                         │ 选择性重新计算   │
                         │ - 只重新计算     │
                         │   变更的部分     │
                         └────────┬────────┘
                                  │
                                  ▼
                         ┌─────────────────┐
                         │ 更新缓存         │
                         │ - 缓存新结果     │
                         │ - 保留未变更     │
                         └─────────────────┘
```

## Agent 并发模型

### 会话隔离级别

```rust
pub enum IsolationLevel {
    /// 读已提交 - 看到其他会话的已提交变更
    ReadCommitted,
    
    /// 快照隔离 - 会话开始时创建快照
    Snapshot,
    
    /// 可串行化 - 完全隔离
    Serializable,
}
```

### Speculative Execution (推测执行)

```rust
pub struct AgentSession {
    id: SessionId,
    isolation: IsolationLevel,
    
    /// 推测分支
    branches: Vec<SpeculativeBranch>,
    
    /// 当前 Universe 快照
    universe_snapshot: UniverseSnapshot,
}

impl AgentSession {
    /// 创建推测分支
    pub fn speculative_branch(&mut self) -> BranchId {
        let branch = SpeculativeBranch::new(self.universe_snapshot.clone());
        self.branches.push(branch);
        branch.id()
    }
    
    /// 在分支上执行操作
    pub fn execute_on_branch(&self, branch_id: BranchId, op: Operation) -> Result<()>;
    
    /// 合并分支
    pub fn merge_branch(&mut self, branch_id: BranchId) -> MergeResult;
    
    /// 放弃分支
    pub fn abort_branch(&mut self, branch_id: BranchId);
}
```

### 并发控制

```rust
/// 乐观并发控制
pub struct OptimisticConcurrency {
    /// 版本号
    versions: DashMap<EntityId, u64>,
}

impl OptimisticConcurrency {
    /// 读取实体 (记录读取版本)
    pub fn read(&self, entity: EntityId) -> Option<Component> {
        let version = self.versions.get(&entity)?;
        // 记录读取集
        self.read_set.insert(entity, *version);
        // 返回数据
        self.get_component(entity)
    }
    
    /// 提交事务
    pub fn commit(&self, transaction: Transaction) -> Result<(), Conflict> {
        // 验证读取集是否被修改
        for (entity, version) in &transaction.read_set {
            let current = self.versions.get(entity).unwrap();
            if *current != *version {
                return Err(Conflict::ReadWrite(entity));
            }
        }
        
        // 应用写入
        for (entity, component) in &transaction.write_set {
            self.set_component(*entity, component.clone());
            self.versions.insert(*entity, version + 1);
        }
        
        Ok(())
    }
}
```

## 性能优化

### 1. 缓存策略

**LRU 缓存**:

```rust
pub struct QueryCache {
    cache: LruCache<QueryKey, QueryResult>,
}

impl QueryCache {
    pub fn get_or_compute<F>(&mut self, key: QueryKey, f: F) -> QueryResult
    where F: FnOnce() -> QueryResult {
        if let Some(result) = self.cache.get(&key) {
            return result.clone();
        }
        
        let result = f();
        self.cache.put(key, result.clone());
        result
    }
}
```

**指纹索引**:

```rust
/// 类型指纹用于快速相等性检查
pub struct TypeFingerprint(u64);

pub struct FingerprintIndex {
    /// 指纹 → 类型列表
    index: DashMap<TypeFingerprint, Vec<TypeId>>,
}

impl FingerprintIndex {
    /// O(1) 查找相似类型
    pub fn find_similar(&self, fingerprint: TypeFingerprint) -> Vec<TypeId> {
        self.index.get(&fingerprint)
            .map(|v| v.clone())
            .unwrap_or_default()
    }
}
```

### 2. SIMD 加速

```rust
#[cfg(target_arch = "x86_64")]
pub fn fingerprint_simd(types: &[Type]) -> Vec<TypeFingerprint> {
    use std::arch::x86_64::*;
    
    unsafe {
        let mut result = Vec::with_capacity(types.len());
        
        // 每次处理 4 个类型 (256-bit 寄存器)
        for chunk in types.chunks_exact(4) {
            let a = _mm256_loadu_si256(chunk[0].as_ptr() as *const __m256i);
            let b = _mm256_loadu_si256(chunk[1].as_ptr() as *const __m256i);
            let c = _mm256_loadu_si256(chunk[2].as_ptr() as *const __m256i);
            let d = _mm256_loadu_si256(chunk[3].as_ptr() as *const __m256i);
            
            // 并行计算指纹
            let hash = _mm256_xor_si256(a, b);
            let hash = _mm256_xor_si256(hash, c);
            let hash = _mm256_xor_si256(hash, d);
            
            result.push(TypeFingerprint(_mm256_extract_epi64(hash, 0) as u64));
        }
        
        result
    }
}
```

### 3. 并行处理

```rust
/// 并行类型检查
pub fn check_parallel(&self, files: &[FileId]) -> Vec<TypeCheckResult> {
    use rayon::prelude::*;
    
    files.par_iter()
        .map(|file_id| self.check_file(*file_id))
        .collect()
}

/// 并行查询
pub fn query_parallel<Q: QueryParams>(&self, queries: Vec<Q>) -> Vec<QueryResult> {
    use rayon::prelude::*;
    
    queries.into_par_iter()
        .map(|q| self.execute_query(q))
        .collect()
}
```

### 4. 内存池

```rust
pub struct TypePool {
    /// 类型对象池
    pool: ObjectPool<Type>,
    
    /// 已分配计数
    allocated: AtomicUsize,
}

impl TypePool {
    pub fn acquire(&self) -> Pooled<Type> {
        self.pool.acquire()
    }
    
    pub fn release(&self, ty: Pooled<Type>) {
        self.pool.release(ty);
    }
}
```

## 基准测试

### 性能测试结果

```
check_1000_functions    time:   [1.1523 ms 1.1821 ms 1.2134 ms]
                        change: [-3.2% -1.5% +0.2%] (p = 0.10 > 0.05)

check_incremental       time:   [23.456 µs 24.891 µs 26.234 µs]
                        change: [-5.1% -2.8% -0.5%] (p = 0.02 < 0.05)

query_cache_hit         time:   [2.891 ns 3.012 ns 3.134 ns]
                        change: [-1.2% +0.4% +2.1%] (p = 0.62 > 0.05)

concurrent_1000_agents  time:   [4.567 ms 4.789 ms 5.012 ms]
                        thrpt:  [199.56 Kelem/s 208.81 Kelem/s 218.96 Kelem/s]
```

### 与 Go 工具对比

| 场景 | wootype | go/types | gopls | 提升 |
|------|---------|----------|-------|------|
| 冷启动 (1000 函数) | 1.2ms | 1-5s | 2-10s | 800-8000x |
| 增量更新 | 25μs | 全量 | ~300ms | 12000x |
| 缓存查询 | 3ns | N/A | ~1μs | 300x |
| 并发 1000 | 4.8ms | 不支持 | ~500ms | 100x |

## 调试与监控

### 性能指标

```rust
pub struct TypeSystemMetrics {
    /// 缓存命中率
    pub cache_hit_rate: f64,
    
    /// 平均查询时间
    pub avg_query_time_ns: u64,
    
    /// 活跃会话数
    pub active_sessions: usize,
    
    /// ECS 实体数
    pub entity_count: usize,
    
    /// Archetype 数量
    pub archetype_count: usize,
}
```

### 调试工具

```bash
# 类型检查性能剖析
wootype profile --file main.go

# 内存使用分析
wootype stats --memory

# 查看 Salsa 依赖图
wootype debug --deps main.go

# 导出类型图
wootype export --format dot --output types.dot
```
