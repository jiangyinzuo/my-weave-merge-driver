# 当前实现状态

`strict-weave` 提供 Git merge driver 和可选的 `driver prepare` 全局预分析。Git 管理 merge、rebase、cherry-pick、stash、index、continue 和 abort；本工具不包装这些操作，也不提供独立 diff 命令。

## 合并与展示

- **行级基线：** 对原始三方文本执行一次 `git merge-file --diff3/--zdiff3`，同时获取判定和展示。保留普通 Git 会报的行级冲突，不允许 entity 分析或分区合并消除它。依据见 [diff3-vs-zdiff3.md](docs/diff3-vs-zdiff3.md)。
- **严格 entity 规则：** 同一 entity 双方修改即冲突，包括修改不同行和相同最终内容。优先保留 weave 拒绝，否则复用其变化分类；仅对未覆盖区域补充原始字节检查。无法可靠分析时保守冲突或明确报错。
- **粒度与语言：** 使用 weave 的语言 registry，覆盖其 34 个 code grammar、84 个扩展名。class、namespace 等按顶层 entity 判定，不独立分析内部方法；具体支持边界见 [languages.md](docs/languages.md)。
- **冲突块：** 自生成块默认把三方逐 byte 相同的前后文移到块外，保留中间三方文本及 marker；双方相同但不同于 base 的修改仍在块内。`--zdiff3` 选择 Git 行级展示风格，自生成块在两种模式下使用相同裁剪规则。见 [conflict-rendering.md](docs/conflict-rendering.md)。
- **原因：** `Reason(kind, subject, evidence)` 保存父原因和子依据，来源标注为 `git`、`weave`、`analyze`；只对同类、同目标归并。中文固定模板不推断业务意图；默认展示双方动作；可靠原文满足 `ours == theirs != base` 时明确提示双方修改结果相同，`--explain-reasons` 展开全部依据。目录见 [conflict-reasons.md](docs/conflict-reasons.md)，同时通过 `include_str!` 引入 rustdoc。

分析规则集中在 `src/analysis/`：`local.rs` 串联单文件规则，`line.rs`、`upstream.rs`、`raw.rs`、`moves.rs` 分别负责 Git、weave、原文和跨文件移动分析；`partition.rs` 提供可靠原文分区，`global.rs` 管理全局报告。已实现规则、触发条件及边界见 [analysis.md](docs/analysis.md)，同时引入 analysis 模块的 rustdoc。当前没有格式化或语义等价识别。

`src/merge/mod.rs` 负责入口校验和串联；`render.rs` 只读分析结果选择展示，`conflict.rs` 生成并裁剪冲突块。依赖原版 weave-core `a3f501d19601126fefcc40a3ebb764b8d07d39fc` 和 sem-core `0.25.0`，无本地 patch。

## 全局预分析

`driver prepare BASE OURS THEIRS` 读取明确的 revision/tree 快照，记录 tree ID、路径、三方 SHA-256 指纹和严格原因。跨文件移动候选要求 entity 类型、名称及原文（含附着注释）完全一致；歧义候选全部保留。移动与另一侧修改或未知状态相遇时，在相关路径记录审核原因。

结果为只读 JSON，schema v2 / engine `strict-weave-global-v6`，排序确定、原子新建、不覆盖已有文件。driver 校验版本、路径和指纹，失配或损坏时返回 `129` 并保持 ours 不变。全局信息只能增加冲突，不能放宽本地判定。协议见 [global-analysis.md](docs/global-analysis.md)。

## 使用

```sh
cargo build --locked

# 在目标仓库配置；替换为实际可执行文件路径。
git config merge.strict-weave.name 'Strict entity and line merge'
git config merge.strict-weave.driver '/root/my-weave-merge-driver/target/debug/strict-weave driver %O %A %B %P %L --ours-label %X --base-label %S --theirs-label %Y'
```

`.gitattributes` 示例：

```gitattributes
*.go  merge=strict-weave
*.ts  merge=strict-weave
*.tsx merge=strict-weave
```

`%X/%S/%Y` 由 Git 提供，验证版本为 Git 2.53.0；未提供标签时显示“来源未识别”。driver 可附加 `--zdiff3`、`--explain-reasons`。

可选的预分析及接入：

```sh
strict-weave driver prepare BASE_TREE OURS_TREE THEIRS_TREE \
  --output /absolute/path/analysis.json --explain-reasons

# 查看报告并决定继续后，仅为对应的 Git 操作传入结果文件。
STRICT_WEAVE_ANALYSIS=/absolute/path/analysis.json git merge feature/example
```

显式 `--analysis PATH` 优先于环境变量；不提供报告时仅做本地判定。退出码：`0` 无冲突，`1` 需审核，`129` 处理失败，CLI 参数错误为 `2`。prepare 返回 `1` 时仍生成报告，但不会安装 index conflicts 或自动阻止后续 Git 操作。

## 正确性边界

- **Git 可能跳过 driver**，包括同内容双方修改、部分删除和文件 rename。prepare 可提前报告，但不能保证整个 Git 操作绝不漏报 entity 冲突。
- prepare 必须显式调用；工具不推导 merge base、构造虚拟 base 或修改 refs/index/hooks。rebase 每一步、cherry-pick、stash 各阶段的三方快照由调用方确定。
- 指纹只能验证当前路径的输入，不能证明整个 Git 操作与报告的 tree 一致。缺失路径与空文件在报告中区分，但 driver 的空输入无法区分两者；输入经 filters、renormalize 等转换后与报告失配会拒绝读取。
- 拒绝 binary、非 UTF-8、超过 1,000,000 bytes 或已有 conflict marker 的单文件输入。预分析还拒绝 symlink/submodule、非 UTF-8 路径、超限输入；变化文本总量和报告各限 64 MiB，移动候选组合上限为 10000。
- 移动分析不支持跨文件重命名、移动同时编辑、同文件位置/作用域移动和可靠引用分析。解析失败会标明关联不完整；未发现候选不代表没有移动。
- mode 冲突和原始 Git stages 由 Git 管理。冲突区内无末尾换行的文本需补换行容纳 marker。

## 验证

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
```

上述检查均通过。文本测试递归发现同目录的 `a.base / a.ours / a.theirs / a.output`，可附加 `.output-zdiff3`、诊断及配置文件；逐 byte 比较，不自动更新预期。目录形式支持多文件三方快照：先生成只读全局报告，再逐路径调用 driver；`.analysis` 断言完整报告，输出覆盖三侧全部路径，区分缺失与空文件。当前为 **105 组单文件、7 组多文件案例，共 186 次 driver CLI 组合运行**，包括 18 组嵌套 entity 和 4 组双方新增行用例。

其它测试覆盖普通 Git 冲突包含关系（64 组三方组合）、冲突块原文还原、entity 身份和原因模型、语言覆盖、全局分析及真实 Git 流程。完整源码文本来自 fixture，少量辅助文本使用 Rust 常量。入口见 [testing.md](docs/testing.md) 和 [嵌套用例索引](docs/nested-entity-fixtures.md)。
