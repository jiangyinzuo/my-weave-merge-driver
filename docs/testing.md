# 文本 fixture 测试

所有案例平铺在 [tests/fixtures/](../tests/fixtures/)，框架扫描 `*.base` 自动发现用例。新增案例无需修改 Rust 代码。

```text
tests/fixtures/
  a.base
  a.ours
  a.theirs
  a.output
  disjoint.go.base
  disjoint.go.ours
  disjoint.go.theirs
  disjoint.go.output
  adjacent.ts.base
  adjacent.ts.ours
  adjacent.ts.theirs
  adjacent.ts.output
  adjacent.ts.output-zdiff3
```

`a` 和 `disjoint.go` 是两个独立用例。四个文件分别保存 base、ours、theirs 和预期合并结果。允许空文件；空侧不能用说明文字代替。

## 自动发现与比较

每个案例在独立临时目录中调用真实的 `strict-weave driver`，比较写回 ours 的结果与 `.output`，逐 byte 比较，不忽略空白、换行风格或文件末尾换行。测试不会修改 fixture 输入，也不会自动更新预期。

`.output-zdiff3` 是可选的额外断言。存在时，框架用原始三份输入在新的临时目录中再次运行 `driver --zdiff3`，比较该文件；原有 `.output` 的默认模式仍必测。不存在时只测试默认模式。两种模式共用由 `.output` 或 `.exit` 决定的预期退出码，防止展示选项改变冲突判定。

四个文件即可运行。默认将用例名作为源码路径：`disjoint.go` 使用 Go 解析器，`independent.ts` 使用 TypeScript 解析器；`a` 没有扩展名，按不支持的语言保守处理。要保留短名称 `a` 又指定语言，可添加 `a.path`，内容例如 `src/calc.go`。

固定版本标签如下，预期输出使用同样的标签：

```text
ours: feature/ours
base: base-commit
theirs: feature/theirs
```

marker size 固定为 7。框架默认从 `.output` 推导预期退出码：存在行首 `<<<<<<<` 时为 `1`，否则为 `0`。这只用于测试预期，不参与产品的冲突判定。

可选附加文件：

| 文件 | 用途 |
| --- | --- |
| `a.path` | 指定传给 driver 的源码路径，用于选择语言；默认是用例名 |
| `a.exit` | 显式指定预期退出码，覆盖默认推导；例如 `129` 表示拒绝处理输入 |
| `a.stderr` | 逐 byte 检查终端诊断；空文件表示必须没有诊断，不存在则不比较 stderr |
| `a.output-zdiff3` | 可选；额外检查 `--zdiff3` 的输出，不能替代必需的 `.output` |
| `a.stderr-zdiff3` | 可选；指定 zdiff3 模式的诊断，否则沿用 `.stderr`；必须同时有 `.output-zdiff3` |

例如已有 conflict marker 的输入会被拒绝：`.exit` 写 `129`，`.output` 与 `.ours` 完全相同，证明错误处理没有覆盖输入。

## 运行

```sh
cargo test --locked --test fixtures -- --nocapture
```

只运行一个用例：

```sh
FIXTURE=disjoint.go cargo test --locked --test fixtures -- --nocapture
```

`FIXTURE=adjacent.ts` 会同时检查默认和 zdiff3 输出。该案例在 zdiff3 模式下将未改动的 `existing()` 及共同空行留在块外，仍保留 `alpha()` / `beta()` 的冲突，退出码保持 `1`。

输出不一致时，框架显示首个不同 byte 的位置、长度和文本 diff，并将实际输出留在 `target/fixture-failures/`。预期文件缺失、孤立输入、未知后缀、无测试用例或指定了不存在的用例，都视为失败。文件按名称排序，失败会汇总，不会因首个不匹配就跳过其它完整用例。若差异仅在 CRLF 等不可见字符，byte 位置与长度仍会指出差异。

zdiff3 的失败产物名称包含 `.zdiff3`，不会覆盖默认模式的诊断产物。

修改预期前，应检查差异是否符合需求。没有自动接受当前输出的“更新快照”模式。

## 其它测试

- [tests/merge.rs](../tests/merge.rs)：补充严格判定、确定性、语法错误、重复名称、CRLF、TSX、容器粒度等检查；枚举 64 组三方输入，对照真实的普通 `git merge-file`，验证其冲突未被消除。
- [tests/git_workflow.rs](../tests/git_workflow.rs)：使用临时 Git 仓库，验证 driver 调用、版本标签、未解决 index stages、全局预分析只读采集、读取移动关联、Git 原生 abort，以及错误时不修改输入。专门展示双方文件相同时，预分析会报冲突但原生 Git 可跳过 driver 的边界。

- [tests/analysis.rs](../tests/analysis.rs)：验证全局关联的确定性、歧义保留、复制/重命名边界、解析降级、全局结果只能增加冲突、结果只读及旧结果拒绝。

运行全部验证：

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```
