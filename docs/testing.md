# 测试与 fixture 维护

测试分为合并核心、全局分析、fixture 和真实 Git 操作四类。fixture 按场景分组在 [tests/fixtures/](../tests/fixtures/)，框架递归扫描 `*.base` 文件或目录自动发现用例；新增文本案例通常只需增加输入和预期文件。

常用入口：

```sh
cargo test -j 2 --offline --locked
cargo test -j 2 --offline --locked --test fixtures
cargo test -j 2 --offline --locked --test operations
FIXTURE=entities/disjoint.go cargo test -j 2 --offline --locked --test fixtures text_fixtures
```

源码 fixture 按 `entities/`、`languages/`、`insertions/`、`nested/`、`layout/`、`moves/`、`multi-file/`、`encoding/` 和 `fallback/` 分类。合并核心、分析测试和集成测试共用这些文本，不另建集成测试 fixture 副本；Rust 常量只用于很短的仓库初始化辅助文本。

## 单文件 fixture

同目录放置以下文件即可定义用例 `a`：

```text
a.base
a.ours
a.theirs
a.output
```

三侧允许为空。默认用例名也是源码路径；通常使用带扩展名的名称选择语言。`.output` 是严格三方合并结果，逐 byte 比较，不忽略空白、换行或文件末尾换行。输出包含冲突 marker 时，默认预期退出码为 `1`，也可以用 `a.exit` 明确指定。

`.output-zdiff3` 是可选的第二种展示断言；它只改变冲突块边界，不改变严格冲突判定。`.stderr`、`.stderr-details` 和 `.stderr-zdiff3` 分别断言普通或详细诊断。`.options` 可设置 marker 大小和三方标签，`.path` 可指定实际源码路径，`.entities` 可校验上游 parser 提取的 entity 类型和名称。

测试直接调用生产的 `merge::merge_with_style`、`analysis::analyze` 和诊断函数，不通过 Git driver。这样 fixture 只验证确定性的三方输入、分析结果和渲染结果。

## 多文件 fixture

目录值输入表示完整快照：

```text
case.base/
case.ours/
case.theirs/
case.output/
```

目录中的相对路径就是源码路径；缺少路径表示文件不存在，空文件仍是存在的文件。`.analysis` 保存完整全局分析报告，`.stderr`、`.output-zdiff3` 等目录保存逐路径预期。测试先对三棵完整快照运行一次全局分析，再对路径应用同一份报告，验证跨文件移动和重命名候选不会依赖先前合并结果。

## 真实 Git 操作

`tests/operations.rs` 是集成测试入口，用例按操作放在 [tests/integration/](../tests/integration/)。测试在临时目录创建真实 Git 仓库、branch 和 commit，调用实际 strict-weave 命令，覆盖：

- `strict-weave merge TARGET`
- `strict-weave cherry-pick COMMIT`
- `strict-weave rebase UPSTREAM`
- `strict-weave rebase --continue|--abort`
- `strict-weave stash apply|pop`
- `--plan -o FILE` 的只读模式
- `--apply FILE` 的计划校验和过期拒绝
- clean 结果、原生行级冲突、严格 entity 冲突、标准 index stages，以及多 commit rebase 的逐步 `--continue` 和 `--abort` 状态
- merge/cherry-pick/rebase 残留报告及损坏报告不影响下次 merge；清除旧报告前后的冲突文件和 index stages 完全相同；旧 clean 计划不会隐藏新冲突，旧冲突计划不会影响新 clean 结果
- 人工解决后，后续 replay 使用新 HEAD 的完整 tree 识别跨文件移动，并保存正确的三方 tree ID
- rebase 首次冲突后停止预演并列出 pending commit，拒绝修改过的计划；提交、branch 更新和中止失败后的重试，不重复重放，不将未完整应用的步骤当作已解决
- 从子目录保存和应用相对路径计划，仍分析仓库完整 tree；损坏或被修改的计划不能取消冲突
- 分叉后的 clean merge/cherry-pick、commit parents 和 author/message；原生行级冲突及双方相同修改复用 fixture 输出逐 byte 断言
- 脏工作区、属性转换、rename、文件/目录冲突、binary 和 symlink 在应用前拒绝；原生 stash apply 失败时保留 stash 和本地修改

测试要求工作区和 index 的初始状态干净，并确认计划绑定的 HEAD、index、仓库路径、target、三方 tree 和分析报告。只读 plan 不应修改工作区、index、HEAD 或 refs；apply 失败也不能消费旧计划。

单文件输入通过 `include_str!` 复用，跨文件输入用公共 helper 将 `multi-file/*.base/ours/theirs` 复制到仓库后提交。人工解决结果优先复用已有文本；展示测试也可复用 `.output-zdiff3`，只替换实际 commit 标签。增加端到端场景时，只新增 Git 操作和状态断言，避免再次维护相同源码与输出。

## 验证

```sh
cargo fmt --all -- --check
cargo clippy -j 2 --offline --locked --all-targets -- -D warnings
cargo test -j 2 --offline --locked
RUSTDOCFLAGS="-D warnings" cargo doc -j 2 --offline --locked --no-deps
```

fixture 预期不会自动更新。发现差异时先阅读三方输入和实际输出，再手工修改对应 `.output` 或报告文件。
