# 测试与 fixture 维护

driver 文本案例按场景分组在 [tests/fixtures/](../tests/fixtures/)，框架递归扫描 `*.base` 文件或目录自动发现用例。新增案例无需修改 Rust 代码。

常用入口：

```sh
cargo test --locked --test fixtures
FIXTURE=entities/disjoint.go cargo test --locked --test fixtures -- --nocapture
FIXTURE=multi-file/modify-vs-move cargo test --locked --test fixtures -- --nocapture
cargo test --locked --test integration
```

源码 fixtures 按 `entities/`、`languages/`、`insertions/`、`nested/`、`layout/`、`moves/`、`multi-file/`、`encoding/`、`fallback/`、`git-baseline/` 分类。完整源码必须来自 fixture，不在 Rust 中拼接生成；几行辅助文本（非法 JSON、Git attributes、状态标记等）可使用 Rust 常量。

人工阅读可从[嵌套 entity 索引](nested-entity-fixtures.md)、[双方新增行](diff3-vs-zdiff3.md#双方新增行的用例)和下方 include 对照开始。精确案例/CLI 运行数量以测试输出为准，不在各专题重复维护。

## C++ include 实验

`languages/includes/` 保存三方源码、默认/zdiff3 输出及默认/详细诊断。下表中的 Git 结果来自对同一输入运行普通 `git merge-file -p`；没有读取头文件内容或运行 C++ 预处理器。

| 案例 | 变化 | Git | driver 当前结果 |
| --- | --- | --- | --- |
| `replace.cpp` | 同一个 include 改成两个不同头文件 | conflict | `LINE_CONFLICT` + `UNMODELED_BOTH_CHANGED` |
| `add.cpp` | 同一位置分别新增 `<string>` / `<map>` | conflict | 同上 |
| `delete-modify.cpp` | 删除 include / 修改该头文件名 | conflict | 同上，删除侧为空 |
| `disjoint.cpp` | 修改相隔多行的不同 include | clean | `UNMODELED_BOTH_CHANGED`，同一非实体区域双方变化 |
| `identical-add.cpp` | 双方新增相同的 `<string>` | clean | `UNMODELED_BOTH_CHANGED`，相同结果仍保留冲突 |
| `include-vs-function.cpp` | 一边新增 include，另一边改函数返回值 | clean | clean，两项修改均保留 |
| `conditional.cpp` | 双方修改同一 `#if` 分支中的 include | conflict | 行级冲突、weave 的 `file_header` 拒绝，以及非实体区域双方变化 |
| `between.cpp` | 两个 function 之间的 include 被双方修改 | conflict | 行级冲突及非实体区域冲突，标明相邻两个 function |
| `trailing.cpp` | 最后一个 function 之后的 include 被双方修改 | conflict | 行级冲突及非实体区域冲突，标明文件末尾和前方 function |
| `headers-only.cpp` | 文件仅含 include，双方修改头文件名 | conflict | 行级冲突、weave 的 `(file)` 拒绝，以及 `ENTITY_ANALYSIS_UNAVAILABLE` 整文件回退 |

普通 include 区域在可靠分区中并非独立 function/entity。内部 key（如 `gap:0`）只用于身份关联，提示改为“文件开头、function answer 之前的非实体区域被双方修改”；文件中间和末尾分别根据相邻 entity 定位。条件编译案例中的 weave `file_header` 无法可靠关联到本地 entity，故其拒绝与原文区域依据分别保留。仅 include 的文件没有可供当前分区使用的 entity，会保守回退；不是 C++ 语言不受支持。

这批案例默认与 zdiff3 输出逐 byte 相同；有未修改函数的案例均将函数留在冲突块外。实际输出可查看 [replace.cpp.output](../tests/fixtures/languages/includes/replace.cpp.output)、[disjoint.cpp.output](../tests/fixtures/languages/includes/disjoint.cpp.output) 和 [headers-only.cpp.stderr-details](../tests/fixtures/languages/includes/headers-only.cpp.stderr-details)。这些断言固定当前行为，不意味着已实现 include 依赖、条件分支可达性或业务语义分析。

```sh
FIXTURE=languages/includes/replace.cpp cargo test --locked --test fixtures -- --nocapture
```

## 单文件：自动发现与比较

同目录放置四个文件即可定义用例 `a`：

```text
a.base
a.ours
a.theirs
a.output
```

三侧允许空文件，不能用说明文字代替。默认用例名也是源码路径；通常使用 `disjoint.go` 这样的带扩展名名称，以选择对应语言。

每个案例在独立临时目录中调用真实的 `strict-weave driver`，比较写回 ours 的结果与 `.output`，逐 byte 比较，不忽略空白、换行风格或文件末尾换行。测试不会修改 fixture 输入，也不会自动更新预期。

`.output-zdiff3` 是可选的额外断言。存在时，框架用原始三份输入在新的临时目录中再次运行 `driver --zdiff3`，比较该文件；原有 `.output` 的默认模式仍必测。不存在时只测试默认模式。两种模式共用由 `.output` 或 `.exit` 决定的预期退出码，防止展示选项改变冲突判定。

源码路径默认不含分类目录；无扩展名的 `a` 按不支持的语言保守处理，也可用 `a.path` 指定 `src/calc.go` 等路径。

默认版本标签如下，预期输出使用同样的标签：

```text
ours: feature/ours
base: base-commit
theirs: feature/theirs
```

marker size 默认为 7；可通过 `.options` 覆盖。框架默认从 `.output` 推导预期退出码：存在行首 `<<<<<<<` 时为 `1`，否则为 `0`。这只用于测试预期，不参与产品的冲突判定。

可选附加文件：

| 文件 | 用途 |
| --- | --- |
| `a.path` | 指定传给 driver 的源码路径，用于选择语言；默认是用例名 |
| `a.options` | 可选 JSON：`marker_size`、`ours_label`、`base_label`、`theirs_label`，缺失字段使用默认值，未知字段拒绝 |
| `a.exit` | 显式指定预期退出码，覆盖默认推导；例如 `129` 表示拒绝处理输入 |
| `a.stderr` | 逐 byte 检查终端诊断；空文件表示必须没有诊断，不存在则不比较 stderr |
| `a.output-zdiff3` | 可选；额外检查 `--zdiff3` 的输出，不能替代必需的 `.output` |
| `a.stderr-details` | 可选；对每种已声明的输出模式额外运行 `--explain-reasons`，逐 byte 检查完整子依据；输出和退出码继续使用该模式原有预期 |
| `a.stderr-zdiff3` | 可选；指定 zdiff3 模式的诊断，否则沿用 `.stderr`；必须同时有 `.output-zdiff3` |

例如已有 conflict marker 的输入会被拒绝：`.exit` 写 `129`，`.output` 与 `.ours` 完全相同，证明错误处理没有覆盖输入。

自定义 marker 与包含换行的标签也由文件提供，例如 `custom-marker.go.options`：

```json
{
  "marker_size": 11,
  "ours_label": "feature/ours\nINJECT",
  "base_label": "123",
  "theirs_label": "abc cherry-pick"
}
```

`invalid-width.go`、`binary-input.go` 分别保存非法宽度和真实 NUL 输入，并通过 `.exit` 与 `.output` 检查拒绝处理且不改写 ours。`encoding.go`、`crlf-edit.go` 保存真实 CRLF；不要用编辑器归一化这些文件。

## 多文件与跨文件 move

同一命名规则也支持目录。`a.output` 为目录时，该案例按多文件场景运行；`.base / .ours / .theirs` 内的相对路径是实际源码路径，可包含子目录，无需 `.path` 或文件清单。

```text
multi-file/
  modify-vs-move.base/
    src/source.go
    src/unchanged.go
  modify-vs-move.ours/
    src/source.go
    src/unchanged.go
    lib/target.go          # 存在但为空，与文件不存在不同
  modify-vs-move.theirs/
    src/source.go
    src/unchanged.go
    lib/target.go          # 移动后的 function
  modify-vs-move.output/
    src/source.go
    src/unchanged.go
    lib/target.go
  modify-vs-move.stderr/   # 相同路径结构，逐文件诊断
  modify-vs-move.stderr-details/
  modify-vs-move.output-zdiff3/
  modify-vs-move.analysis  # 完整预期 JSON 报告
```

每种模式先读取三份完整快照，调用与 prepare 相同的 `analysis::analyze` 并保存只读报告，再逐路径调用真实 `strict-weave driver --analysis ...`。所有文件使用同一份报告和原始输入，不把先合并的结果作为后续分析输入。未变化路径不会出现在报告中，对这些路径直接运行本地 driver。结束后检查报告未被修改。

这套测试验证全局报告与逐文件 driver 的配合，不模拟原生 Git 对删除、rename、同 blob 的调度；真实操作由端到端测试覆盖。

输入、输出约定：

- 某侧目录中没有某路径，表示该文件不存在；存在的空文件有自己的空文本指纹。传给单文件 driver 时，不存在的一侧才转换为空文本。
- 三个输入快照必须显式存在。整份快照为空时，用零 byte 的 `a.base`（或 `.ours / .theirs`）文件表示，便于 Git 跟踪；`empty-base` 提供示例。不能省略整个快照。
- `.output` 必须覆盖三侧路径并集，不允许缺少或多出文件。已删除路径也放一个空的预期文件，断言的是 driver 输出文本，不是最终 Git tree 的路径存在性。
- 可选 `.output-zdiff3`、`.stderr`、`.stderr-zdiff3`、`.stderr-details` 也使用目录；一旦提供，必须覆盖全部路径。空诊断文件明确断言没有诊断。
- `.exit` 可选为目录，用源码相对路径保存需要覆盖的退出码；其余路径按默认 `.output` 是否有 marker 推导。默认和 zdiff3 共用退出码。
- `.options` 仍是 JSON 文件，作用于整组 driver；多文件案例不支持 `.path`。

`.analysis` 必须提供，逐 byte 比较完整报告，包括所有原因、移动候选、歧义数量、另一侧状态、警告、缺失/空文件指纹及相关路径。三方标识固定为 `fixture:base / fixture:ours / fixture:theirs`，不需要构造 Git commit。它使用当前产品的报告序列化格式；分析生成失败会令该案例失败。

代表性用例包括 `modify-vs-move`、`pure-move`、`copy`、`ambiguous-move`、`rename-modify`、`rename-format-file`、`rename-both-move`。其余用例覆盖另一侧删除/解析失败、词边界、sentinel、grammar、注释/字面量空白和 Python 嵌套变化等匹配边界，详细预期直接查看各 `.analysis` 与输出目录。

## 过滤与失败排查

`FIXTURE` 接受完整相对路径；文件名全局唯一时也可使用短名称。重名时必须提供相对路径。只声明默认输出时测试一次；有 `.output-zdiff3` 时额外测该模式；有 `.stderr-details` 时每种模式再测详细诊断。

逐 byte 比较不忽略空白、CRLF 或末尾换行。预期缺失、孤立输入、未知后缀、未找到用例或过滤器不匹配都失败；框架汇总失败，不因首个不匹配跳过后续完整案例。

失败时显示首个不同 byte、文本长度与 diff，实际产物留在 `target/fixture-failures/`，保留 fixture 子目录以及 zdiff3/details 模式后缀。多文件报告差异保存为 `<case>.actual-analysis`，源码结果按场景内路径保存。

**没有自动更新预期的开关。** 阅读实际结果与需求、确认差异正确后，再手工修改 fixture。CRLF/NUL 等测试保存真实字节，不应由编辑器归一化。

## 其它测试

共享源码读取入口是 [tests/common/mod.rs](../tests/common/mod.rs)，单/多文件框架分别在 [tests/fixtures.rs](../tests/fixtures.rs) 和 [fixture_support/multi.rs](../tests/fixture_support/multi.rs)。框架自身检查缺失/空快照、预期路径覆盖和递归发现规则。

| 测试 | 断言 |
| --- | --- |
| [merge.rs](../tests/merge.rs) | weave 复用、原文补充、确定性、方向对称；固定 8×8 三方组合对照普通/diff3/zdiff3 Git，验证冲突包含关系并保留行级原因 |
| [conflict_blocks.rs](../tests/conflict_blocks.rs) | 沿三种 marker 分支还原原文、共同文本裁剪、换行与强制冲突 |
| [analysis.rs](../tests/analysis.rs) | 全局候选、歧义、格式/重命名边界、报告只读及过期输入拒绝 |
| [reasons.rs](../tests/reasons.rs) | 全部父子原因、双方动作、weave 拒绝、来源、归并、结构校验与序列化 |
| [languages.rs](../tests/languages.rs) | 从上游 registry 派生 grammar/扩展名覆盖，逐语言验证 entity 与跨文件候选 |
| [git_workflow.rs](../tests/git_workflow.rs) | 原生 driver 标签/index stages、prepare 只读采集、报告消费、Git 跳过 driver 的边界及错误不改写输入 |

Git 的 8×8 对照只组合 `git-baseline/` 中固定源码，不生成随机源码。fixture 和结构化性质测试互补，不能只因输出快照通过就取消包含关系检查。

完整验证：

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
```

## 真实 Git CLI 端到端测试

入口为 [tests/integration.rs](../tests/integration.rs)，实现按操作放在 [tests/integration/](../tests/integration/)。运行：

```sh
cargo test --locked --test integration
cargo test --locked --test integration rebase::
cargo test --locked --test integration cross_file_move -- --nocapture
```

每个测试在 `/tmp` 创建独立的真实 Git 仓库，配置测试身份，复制已有 fixtures，实际执行 checkout/branch/add/commit 等命令，再调用编译出的 `strict-weave` 或原生 `git`。Git 进程没有 mock；pull 的 remote 也是本地临时仓库，不访问网络。不读取用户 global/system Git 配置，不继承外部 Git 操作环境或分析报告；editor 使用确定性的短命令。测试结束自动清理临时目录。仓库名包含空格和单引号，同时验证 shell 参数引用。

| 文件 | 验证内容 |
| --- | --- |
| `support.rs` | 隔离仓库、进程环境、复制 fixture、Git commit、原文及状态断言 |
| `merge.rs` | 独立 entity 合并、同 function 不同行冲突、同 blob 修改、行级冲突下限、跨文件移动、ff/no-ff/squash/no-commit/abort、dirty/多 base/未知参数拒绝、配置隔离、Git alias、linked worktree、子目录以外的冲突 |
| `rebase.rs` | 每步报告、后续步骤冲突、continue 重新检查、skip/abort、交互式 reorder/drop/edit/squash/fixup、edit-todo、root/onto/branch、rebase-merges 的拓扑和 merge 步骤检查、editor 失败 |
| `pull_stash.rs` | 本地 fetch 后 merge/rebase/interactive/merges/ff-only、fetch 后冲突停止、stash apply/pop、--index、工作区/index 双层修改、stash 第三 parent、移动到未跟踪文件、从子目录执行 |
| `native.rs` | 直接执行 `git merge/rebase/cherry-pick/stash pop`，真实调用 driver 产生 marker 和未解决 index stages，并验证 abort 或 stash 保留 |

仓库历史和预期状态由 Rust 表达，源码与最终文件读取已有 fixture。断言包括退出码、HEAD/父 commit/分支、文件原文、index tree/stages、stash 引用及报告；区分“预分析停止、尚未应用步骤”和“原生 Git 已产生未解决冲突”。原生行级 driver 对照用于确认严格规则确实拦截了 Git 会接受的修改。
