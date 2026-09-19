# 文本 fixture 测试

driver 文本案例按场景分组在 [tests/fixtures/](../tests/fixtures/)，框架递归扫描 `*.base` 文件或目录自动发现用例。新增案例无需修改 Rust 代码。

```text
tests/fixtures/
  entities/       # 同一实体修改、独立修改、完整范围展示
    disjoint.go.base
    disjoint.go.ours
    disjoint.go.theirs
    disjoint.go.output
  languages/      # 上游各 code grammar 的示例
    includes/     # C++ #include 修改、新增、删除、条件编译及独立修改对照
  insertions/     # 双方新增行：不同内容、相同内容、共同边界、不同位置
  nested/         # C++ / Python 非顶层 entity、类方法和宏；见专门的用例索引
  layout/         # 相邻插入、增删、重排、非实体区域
    adjacent.ts.base
    adjacent.ts.ours
    adjacent.ts.theirs
    adjacent.ts.output
    adjacent.ts.output-zdiff3
  moves/          # 跨文件移动分析复用的文本
  multi-file/     # 整组三方快照、全局分析报告及各路径的 driver 输出
  encoding/       # CRLF、末尾无换行
  fallback/       # 不支持语言、语法错误、重复名称、非法输入
  git-baseline/   # 真实 Git 8×8 对照使用的版本
```

`a` 和 `disjoint.go` 是两个独立用例。四个文件分别保存 base、ours、theirs 和预期合并结果。允许空文件；空侧不能用说明文字代替。

非顶层 entity 的 18 组人工检视案例及当前展示范围见 [nested-entity-fixtures.md](nested-entity-fixtures.md)。

`insertions/` 的 4 组双方新增行用例均包含 `.output`、`.output-zdiff3` 和 `.stderr-details`，共验证 16 次 CLI 运行。具体输出和与原生 Git 的区别见 [diff3-vs-zdiff3.md](diff3-vs-zdiff3.md#双方新增行的用例)。

## C++ include 实验

`languages/includes/` 的 10 组单文件案例保存三方源码、默认/zdiff3 输出、默认/详细诊断，共 40 次 CLI 断言。下表中的 Git 结果来自对同一输入运行普通 `git merge-file -p`；没有读取头文件内容或运行 C++ 预处理器。

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

## 自动发现与比较

每个案例在独立临时目录中调用真实的 `strict-weave driver`，比较写回 ours 的结果与 `.output`，逐 byte 比较，不忽略空白、换行风格或文件末尾换行。测试不会修改 fixture 输入，也不会自动更新预期。

`.output-zdiff3` 是可选的额外断言。存在时，框架用原始三份输入在新的临时目录中再次运行 `driver --zdiff3`，比较该文件；原有 `.output` 的默认模式仍必测。不存在时只测试默认模式。两种模式共用由 `.output` 或 `.exit` 决定的预期退出码，防止展示选项改变冲突判定。

四个文件即可运行。默认将不含分类目录的用例名作为源码路径：`disjoint.go` 使用 Go 解析器，`independent.ts` 使用 TypeScript 解析器；`a` 没有扩展名，按不支持的语言保守处理。要保留短名称 `a` 又指定语言，可添加 `a.path`，内容例如 `src/calc.go`。

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

这套测试直接验证全局分析与多个文件 driver 的配合，不执行原生 `git merge`，也不模拟 Git 对删除、rename、同 blob 等路径的调度。driver 可能被 Git 跳过的边界继续由 `tests/git_workflow.rs` 验证。

输入、输出约定：

- 某侧目录中没有某路径，表示该文件不存在；存在的空文件有自己的空文本指纹。传给单文件 driver 时，不存在的一侧才转换为空文本。
- 三个输入快照必须显式存在。整份快照为空时，用零 byte 的 `a.base`（或 `.ours / .theirs`）文件表示，便于 Git 跟踪；`empty-base` 提供示例。不能省略整个快照。
- `.output` 必须覆盖三侧路径并集，不允许缺少或多出文件。已删除路径也放一个空的预期文件，断言的是 driver 输出文本，不是最终 Git tree 的路径存在性。
- 可选 `.output-zdiff3`、`.stderr`、`.stderr-zdiff3`、`.stderr-details` 也使用目录；一旦提供，必须覆盖全部路径。空诊断文件明确断言没有诊断。
- `.exit` 可选为目录，用源码相对路径保存需要覆盖的退出码；其余路径按默认 `.output` 是否有 marker 推导。默认和 zdiff3 共用退出码。
- `.options` 仍是 JSON 文件，作用于整组 driver；多文件案例不支持 `.path`。

`.analysis` 必须提供，逐 byte 比较完整报告，包括所有原因、移动候选、歧义数量、另一侧状态、警告、缺失/空文件指纹及相关路径。三方标识固定为 `fixture:base / fixture:ours / fixture:theirs`，不需要构造 Git commit。它使用当前产品的报告序列化格式；分析生成失败会令该案例失败。

已有 33 组多文件场景，其中原有移动场景：

| 案例 | 检查内容 |
| --- | --- |
| `modify-vs-move` | ours 修改、theirs 移动；子目录路径、未变化文件、缺失与空文件，另有 zdiff3 断言 |
| `move-vs-modify` | ours 移动、theirs 修改，验证方向对称 |
| `pure-move` | 源文件删除、目标新增，另一侧未变；保留候选但不报冲突 |
| `copy` | 源 entity 仍存在，复制不报移动候选 |
| `ambiguous-move` | 两个目标全部保留，源与两个目标都报审核原因 |
| `unknown-opposite` | 另一侧解析失败，显示 unknown 和关联不完整警告，保守报冲突 |
| `empty-base` | 空 base，两侧分别新增文件，不误认为移动 |

另有 18 组疑似重命名移动场景：

| 案例 | 检查内容 |
| --- | --- |
| `rename-modify`、`rename-ours` | 双向修改/重命名移动；源与目标均审核，前者含 zdiff3 断言 |
| `rename-pure`、`rename-copy` | 纯重命名移动仅显示关联；保留源 entity 的复制不产生候选 |
| `rename-ambiguous`、`rename-many-sources` | 所有来源/目标保留，数量包含同名精确移动，不选择唯一配对 |
| `rename-literal-comment`、`rename-docstring` | 字符串和附着文本也可能被替换；不得宣称只改定义名 |
| `rename-recursive-unicode` | Unicode 名称和自引用替换次数 |
| `rename-unknown`、`rename-deleted` | 另一侧无法解析/已删除时保守审核 |
| `rename-grammar-alias`、`rename-cross-grammar` | 同 grammar 的扩展名可匹配；不同 grammar 不作归一化匹配 |
| `rename-format`、`rename-body-edit` | 格式差异通过语法回退匹配；额外内容变化不匹配，仍保留原有冲突 |
| `rename-sentinel`、`rename-word-boundary`、`rename-same-file` | sentinel、非词边界子串、同文件排除 |

另有 7 组格式化边界案例：`rename-format-file`（文件改名移动、格式化、函数轻微改名）、`rename-format-python`（缩进宽度改变）、`rename-indent-structure`（缩进改变嵌套，拒绝匹配）、`rename-literal-whitespace`、`rename-comment-whitespace`、`rename-template-whitespace`（内部空白必须保留）、`rename-format-body-edit`（额外内容编辑不能被格式化掩盖）。

`rename-both-move` 另验证 ours 同名移动、theirs 改名到两个候选目标：旧文件和各目标的两侧 marker 提示、另一侧候选及详细依据、默认/zdiff3、自定义 11 字符 marker，保留全部歧义。

这些案例全部断言默认和详细 stderr，以及 schema v5 / engine v9 的 `.analysis`。报告用 `matched_by` 明确区分精确移动与名称归一化，并检查替换次数、歧义数量和另一侧状态。

新增场景只需添加这些目录和文本，无需改 Rust 测试。可先用空文件起草 `.analysis` 和输出预期，运行后阅读失败 diff 及实际产物，确认符合要求后手工填写；框架没有自动接受结果的开关。

## 运行

```sh
cargo test --locked --test fixtures -- --nocapture
```

只运行一个用例：

```sh
FIXTURE=entities/disjoint.go cargo test --locked --test fixtures -- --nocapture

# 一组多文件的全局报告、默认/zdiff3 输出和详细诊断
FIXTURE=multi-file/modify-vs-move cargo test --locked --test fixtures -- --nocapture
```

过滤器接受完整相对路径；文件名在所有目录中唯一时，也兼容 `FIXTURE=disjoint.go`。重名时必须给出相对路径，避免选错用例。

`FIXTURE=layout/adjacent.ts` 会检查默认、zdiff3 及两者的详细原因模式，共四次独立运行。需要检查诊断的案例提供 `.stderr` 和 `.stderr-details`；空文件断言没有诊断。该案例在 zdiff3 模式下将未改动的 `existing()` 及共同空行留在块外，仍保留 `alpha()` / `beta()` 的冲突，退出码保持 `1`。

输出不一致时，框架显示首个不同 byte 的位置、长度和文本 diff，并将实际输出留在 `target/fixture-failures/`。预期文件缺失、孤立输入、未知后缀、无测试用例或指定了不存在的用例，都视为失败。文件按完整相对路径排序，失败会汇总，不会因首个不匹配就跳过其它完整用例。若差异仅在 CRLF 等不可见字符，byte 位置与长度仍会指出差异。

失败产物保留 fixture 子目录，避免同名案例相互覆盖。zdiff3 的失败产物名称包含 `.zdiff3`，详细模式再加 `.details`，不会互相覆盖。

多文件产物另外保留场景内源码路径，例如 `multi-file/modify-vs-move/src/source.go.actual`；报告差异保存在 `multi-file/modify-vs-move.actual-analysis`。

修改预期前，应检查差异是否符合需求。没有自动接受当前输出的“更新快照”模式。

## 其它测试

完整合并用例共用磁盘上的源码文本，读取入口在 [tests/common/mod.rs](../tests/common/mod.rs)；不在 Rust 中用 `replace`、`format!` 或字符串拼接生成 base / ours / theirs 或预期源码。只涉及文本结果的旧测试已并入自动发现框架，结构化原因和性质断言继续保留。

- `tests/fixtures/`：115 组单文件案例、33 组多文件案例，共 352 次 driver CLI 组合运行（单文件 188 次、多文件 164 次）。Git 的 8×8 对照从 `git-variant-0.ts` 到 `git-variant-7.ts` 读取固定版本，只组合已有文本，不生成源码。
- 少量几行的辅助文本（原因提示、非法 JSON、控制字符、Git attributes 及工作区状态标记）直接使用 Rust 常量，放在对应测试附近；跨测试共用的常量放在 `tests/common/mod.rs`，无需单独建文件。

全局分析和 Git 流程测试复用 `move-source.go`、`move-target.go` 等案例，代码只组织路径、快照与 Git 操作。来源枚举、证据组合、非法缓存字段变异等结构化断言仍由 Rust 表达。

多文件框架位于 `tests/fixture_support/multi.rs`，与单文件案例共用 CLI 执行及比较函数。框架本身还验证缺失快照不能误当空快照、预期路径遗漏/多余必须失败，以及源码文件名不会被误发现为独立案例。

复用规则的回归测试验证：weave 已拒绝的 entity 只保留拒绝依据；尚未拒绝的 entity 使用上游分类执行严格策略；一个 entity 被拒绝不能跳过另一个 entity。`encoding.go` 包含真实 CRLF 字节，验证 weave 归一化后未识别的双方原文变化仍然冲突。

- [tests/languages.rs](../tests/languages.rs)：从 weave 当前支持集与 sem-core code registry 派生测试范围，覆盖 34 个 grammar、84 个扩展名的实体分析及单方修改，逐 grammar 验证跨文件移动；所有源码从 `languages/` 读取。
- [tests/merge.rs](../tests/merge.rs)：读取同一批 fixture，补充上游分类/拒绝、原文补充、确定性等结构化断言；枚举 64 组三方输入，直接运行普通、diff3、zdiff3 三种 `git merge-file`，验证普通冲突包含关系和两种展示模式判定一致，再检查 driver 保留行级原因。
- [tests/git_workflow.rs](../tests/git_workflow.rs)：使用临时 Git 仓库，验证 driver 调用、版本标签、未解决 index stages、全局预分析只读采集、读取移动关联、Git 原生 abort，以及错误时不修改输入。专门展示双方文件相同时，预分析会报冲突但原生 Git 可跳过 driver 的边界。

- [tests/analysis.rs](../tests/analysis.rs)：验证全局关联的确定性、歧义保留、复制/重命名边界、解析降级、全局结果只能增加冲突、结果只读及旧结果拒绝。

- [tests/reasons.rs](../tests/reasons.rs)：父子模型的完整原因目录、全部双方变化组合、全部上游拒绝类型及方向、精确归并、保留歧义、序列化和非法结构检查。来源覆盖 `git`、`weave`、`analyze`，同时验证多条 weave 依据只归属一个来源、weave 无分类时的阻断归属 analyze。原因目录见 [conflict-reasons.md](conflict-reasons.md)。

运行全部验证：

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
```

共同文本裁剪的原文还原检查见 `tests/conflict_blocks.rs`；规则见 [conflict-rendering.md](conflict-rendering.md)。
