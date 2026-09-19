# 文本 fixture 测试

driver 文本案例按场景分组在 [tests/fixtures/](../tests/fixtures/)，框架递归扫描 `*.base` 自动发现用例。新增案例无需修改 Rust 代码。

```text
tests/fixtures/
  entities/       # 同一实体修改、独立修改、完整范围展示
    disjoint.go.base
    disjoint.go.ours
    disjoint.go.theirs
    disjoint.go.output
  languages/      # 上游各 code grammar 的示例
  layout/         # 相邻插入、增删、重排、非实体区域
    adjacent.ts.base
    adjacent.ts.ours
    adjacent.ts.theirs
    adjacent.ts.output
    adjacent.ts.output-zdiff3
  moves/          # 跨文件移动分析复用的文本
  encoding/       # CRLF、末尾无换行
  fallback/       # 不支持语言、语法错误、重复名称、非法输入
  git-baseline/   # 真实 Git 8×8 对照使用的版本
```

`a` 和 `disjoint.go` 是两个独立用例。四个文件分别保存 base、ours、theirs 和预期合并结果。允许空文件；空侧不能用说明文字代替。

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

## 运行

```sh
cargo test --locked --test fixtures -- --nocapture
```

只运行一个用例：

```sh
FIXTURE=entities/disjoint.go cargo test --locked --test fixtures -- --nocapture
```

过滤器接受完整相对路径；文件名在所有目录中唯一时，也兼容 `FIXTURE=disjoint.go`。重名时必须给出相对路径，避免选错用例。

`FIXTURE=layout/adjacent.ts` 会检查默认、zdiff3 及两者的详细原因模式，共四次独立运行。需要检查诊断的案例提供 `.stderr` 和 `.stderr-details`；空文件断言没有诊断。该案例在 zdiff3 模式下将未改动的 `existing()` 及共同空行留在块外，仍保留 `alpha()` / `beta()` 的冲突，退出码保持 `1`。

输出不一致时，框架显示首个不同 byte 的位置、长度和文本 diff，并将实际输出留在 `target/fixture-failures/`。预期文件缺失、孤立输入、未知后缀、无测试用例或指定了不存在的用例，都视为失败。文件按完整相对路径排序，失败会汇总，不会因首个不匹配就跳过其它完整用例。若差异仅在 CRLF 等不可见字符，byte 位置与长度仍会指出差异。

失败产物保留 fixture 子目录，避免同名案例相互覆盖。zdiff3 的失败产物名称包含 `.zdiff3`，详细模式再加 `.details`，不会互相覆盖。

修改预期前，应检查差异是否符合需求。没有自动接受当前输出的“更新快照”模式。

## 其它测试

完整合并用例共用磁盘上的源码文本，读取入口在 [tests/common/mod.rs](../tests/common/mod.rs)；不在 Rust 中用 `replace`、`format!` 或字符串拼接生成 base / ours / theirs 或预期源码。只涉及文本结果的旧测试已并入自动发现框架，结构化原因和性质断言继续保留。

- `tests/fixtures/`：83 组完整三方输入与预期输出；当前共 96 次 CLI 组合运行。Git 的 8×8 对照从 `git-variant-0.ts` 到 `git-variant-7.ts` 读取固定版本，只组合已有文本，不生成源码。
- 少量几行的辅助文本（原因提示、非法 JSON、控制字符、Git attributes 及工作区状态标记）直接使用 Rust 常量，放在对应测试附近；跨测试共用的常量放在 `tests/common/mod.rs`，无需单独建文件。

全局分析和 Git 流程测试复用 `move-source.go`、`move-target.go` 等案例，代码只组织路径、快照与 Git 操作。来源枚举、证据组合、非法缓存字段变异等结构化断言仍由 Rust 表达。

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
