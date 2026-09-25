# strict-weave

`strict-weave` 是一个基于 Git 对象和 index 的严格 entity + 行级合并工具。它不注册 Git merge driver，也不要求 `.gitattributes`；命令本身负责读取完整的 base / ours / theirs tree，执行全局 entity 分析，再让 Git 建立标准冲突状态。

当前公开命令只有受限的：

```sh
strict-weave merge feature/payment
strict-weave cherry-pick <commit>
```

第一阶段要求工作区和 index 干净、只有一个 merge-base，并拒绝未实现的 Git 选项、rename、filters、sparse checkout 和复杂布局变化。分析完成后才修改 Git 状态。冲突时保留标准 index stages，工作区写入 strict-weave 冲突块；可以继续使用原生 Git：

```sh
git add <file>
git merge --continue
git cherry-pick --continue
```

`strict-weave` 不会把不支持的命令或选项静默转交给 Git。直接执行 `git merge`、`git rebase` 等命令不会经过 strict-weave。

全局分析报告保存在当前仓库的 `.git/strict-weave/operation-*/analysis.json`，同时绑定本次三方 tree。报告只用于审计和诊断，Git 的 index、工作区和 refs 仍由操作层按标准方式维护。

## 构建与测试

```sh
cargo test --offline --locked
cargo clippy --offline --locked --all-targets -- -D warnings
cargo fmt --check
```

测试包含 entity、原因模型、语言 registry、文本 fixture，以及真实临时 Git 仓库中的 merge/cherry-pick 流程。

## 文档索引

- [需求与示例](docs/requirement.md)
- [当前分析规则](docs/analysis.md)
- [冲突原因模型](docs/conflict-reasons.md)
- [操作边界](docs/workflows.md)
- [测试说明](docs/testing.md)
- [语言支持](docs/languages.md)
