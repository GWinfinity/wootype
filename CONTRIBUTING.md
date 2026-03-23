# Contributing to Wooftype

## Git 工作流程

### 分支策略

我们使用 Git Flow 风格的分支管理：

- `master`: 稳定版本分支
- `develop`: 开发集成分支
- `feature/*`: 功能分支
- `bugfix/*`: 修复分支
- `release/*`: 发布分支

### 开发流程

1. **创建功能分支**
   ```bash
   git checkout develop
   git pull origin develop
   git checkout -b feature/your-feature-name
   ```

2. **提交更改**
   ```bash
   git add -A
   git commit -m "feat: add new feature description
   
   Detailed description of the changes."
   ```

3. **合并到 develop**
   ```bash
   git checkout develop
   git merge feature/your-feature-name
   git push origin develop
   ```

### 提交信息规范

遵循 Conventional Commits:

- `feat:` 新功能
- `fix:` 修复
- `docs:` 文档
- `style:` 格式调整
- `refactor:` 重构
- `perf:` 性能优化
- `test:` 测试
- `chore:` 构建/工具

### Git 别名

项目配置了常用别名：

```bash
git st      # status
git co      # checkout
git br      # branch
git ci      # commit
git lg      # log --oneline --graph --all
git lg5     # log --oneline -5
```

### 版本标签

- `v0.1.0-phase1` - 阶段一：共存架构
- `v0.2.0-phase2` - 阶段二：服务化 (计划中)
- `v0.3.0-phase3` - 阶段三：语义操作系统 (计划中)

## 开发指南

### 运行测试

```bash
cargo test
```

### 代码检查

```bash
cargo clippy
cargo fmt --check
```

### 提交前检查清单

- [ ] 代码可以编译
- [ ] 测试通过
- [ ] Clippy 无警告
- [ ] 代码已格式化
- [ ] 提交信息符合规范
