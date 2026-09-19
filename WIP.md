# 初步实现进度

当前仅提供 `strict-weave driver`，其中 `driver prepare` 是可选的显式三方预分析阶段。已删除操作级 merge 入口，回退未定型的 diff 实现；不提供 diff、rebase、cherry-pick、stash 等命令。Git 自己管理操作、index、continue 和 abort。

## 已实现

- **独立行级基线：** 用原始三份文本执行普通 `git merge-file`，其冲突不会被 weave 或全局分析消除。diff3 只用于展示。
- **严格实体规则：** 同一实体双方修改必须冲突，包括不同改动行和相同最终内容。无法可靠分析时回退为整文件冲突或明确报错。
- **固定依赖：** weave-core 固定到 `a3f501d19601126fefcc40a3ebb764b8d07d39fc`，sem-core 固定为 `0.25.0`；保留上游明确拒绝的判定。
- **展示：** Go、TypeScript、TSX 支持有限的完整实体展示；类等容器按顶层整体处理。中文固定模板解释原因，来源标签由明确参数或 Git 提供，不推断业务意图。
- **全局预分析：** 显式接收 base / ours / theirs revision 或 tree，记录解析后的 tree ID；读取三份快照的变化文件，复用 driver 严格检查，并记录每个路径三侧的 SHA-256 文本指纹。
- **移动关联：** 分别比较 base → ours、base → theirs，关联跨文件 deleted / added 的实体。首版只匹配类型、名称、原始区域文本完全相同的 entity，含附着注释。保留所有歧义候选及数量，不强行选择唯一关系。
- **修改与移动：** 若另一侧改变源实体或无法确认其未改变，在源、目标路径记录共同审核原因。driver 可以据此增加冲突和移动目标说明。
- **只读结果：** JSON 包含格式及分析算法版本，排序确定；原子新建并设为只读，不覆盖已有结果。各 driver 只读取它，不维护共享可变缓存。
- **输入校验：** driver 检查结果版本、路径、三侧文本指纹；缺失、损坏或不匹配时退出 `129`，保持 ours 不变。全局信息只能增加冲突，不能放宽本地判定。
- **文本 fixture：** 保留同目录 `a.base`、`a.ours`、`a.theirs`、`a.output` 的自动发现和逐 byte 比较，见 [docs/testing.md](docs/testing.md)。
- **可选 zdiff3：** `driver --zdiff3` 开启紧凑行级展示，默认不变。确认原文未变且双方仅在末尾新增不同实体时，可缩小整文件回退；严格实体冲突仍保留完整三方范围。fixture 可增加 `.output-zdiff3` 额外断言，`adjacent.ts` 已覆盖两种模式。

## 使用

构建：

```sh
cargo build --locked
```

在目标仓库配置（替换可执行文件路径）：

```sh
git config merge.strict-weave.name 'Strict entity and line merge'
git config merge.strict-weave.driver '/root/my-weave-merge-driver/target/debug/strict-weave driver %O %A %B %P %L --ours-label %X --base-label %S --theirs-label %Y'
```

`.gitattributes` 示例：

```gitattributes
*.go  merge=strict-weave
*.ts  merge=strict-weave
*.tsx merge=strict-weave
```

`%X/%S/%Y` 由 Git 提供，验证版本 Git 2.53.0；未提供标签时显示“来源未识别”。没有自动安装或全局配置修改。

可选的预分析阶段：

```sh
strict-weave driver prepare BASE_TREE OURS_TREE THEIRS_TREE \
  --output /absolute/path/analysis.json
```

三个版本由调用方明确选定，工具不猜测 base。退出 `0` 表示分析完成且没有命中当前严格内容规则；`1` 表示有需审核项，**结果文件仍已生成**；`129` 表示无法完成。CLI 参数错误退出 `2`。

读取报告并决定如何处理后，将文件路径只传给本次 Git 操作：

```sh
STRICT_WEAVE_ANALYSIS=/absolute/path/analysis.json git merge feature/example
```

也可在 driver 配置中显式传 `--analysis /absolute/path/analysis.json`，它优先于环境变量。不提供分析文件时保持原有单文件判定。不要长期导出该变量或让后续操作复用旧文件。prepare 返回 `1` 时，不应无条件继续 Git；shell 可用 `&&` 阻止继续，但这不等于已创建 index conflicts。

完整协议及限制见 [docs/global-analysis.md](docs/global-analysis.md)。

## 正确性边界

- **Git 跳过 driver 的盲区仍存在。** 同内容双方修改、某些删除、文件 rename 等可不调用 driver。prepare 能提前报告其检查到的风险，但结果文件不能自动安装 index stages，也不能拦截忽略退出码继续执行的 Git。不能宣称全流程已实现“不漏报实体冲突”。
- Git 没有通用的 driver 初始化 hook。prepare 由用户或外部调用方显式运行；工具不执行 Git 操作，不修改 refs、index、hooks、rerere 等配置。
- 指纹确认的是此路径本次收到的三份文本，无法证明整个 Git 操作采用了报告中的三个 tree。不同全局上下文可能含相同局部文本；调用方仍须负责上下文对应。缺失路径与空文件在报告中区分，但 driver 的空输入无法证明路径存在性。
- 不自动选 merge base 或构造虚拟 base。rebase 每一步、cherry-pick 的选定父提交、stash 的多次合并阶段需要各自正确的三份快照，尚无自动接入。renormalize、filters、路径 rename 等导致输入与原始 blob 不一致时，初版明确拒绝。
- binary、非 UTF-8、单文件超过 1,000,000 bytes、已有 conflict marker 的输入拒绝处理。预分析变化文件遇到 symlink/submodule、非 UTF-8 路径或超限也拒绝。三份变化文本及结果文件各限 64 MiB；候选组合超过 10000 时拒绝生成不完整报告。
- 不支持的语言或解析失败会标明移动关联不完整；单文件严格规则仍执行。未实现跨文件重命名、移动同时编辑、同文件位置/作用域移动、可靠引用分析。未识别移动不代表没有移动。
- mode 冲突由 Git 管理。冲突块无末尾换行的侧需补换行以容纳 marker；Git stages 由 Git 保留。
- 独立 diff 继续留在需求讨论阶段，暂不实现或暴露命令。

## 验证

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

以上检查均已通过。测试包含 16 项单文件行为检查（含 64 组三方输入对照普通 Git，并比较两种展示模式的冲突判定）、10 组平铺合并 fixture（adjacent.ts 额外检查 zdiff3）、6 项全局分析检查、7 项真实 CLI/Git 流程检查。覆盖确定性、歧义、复制不误报移动、旧报告拒绝、结果只读/不覆盖、dirty 仓库只读采集、实际 driver 读取移动信息、环境变量与参数优先级、Git 原生 abort，以及双方同内容时 Git 跳过 driver 的边界。另通过临时修改 zdiff3 预期验证了该断言确实失败、默认模式仍通过，恢复预期后两种模式均通过。

## 下一步

1. 扩展三方预分析和真实 Git 场景的可读 fixture，确认复杂基准、路径变化和内容转换的接入协议。
2. 在保留原始三方内容及行级冲突的前提下，逐步缩小整文件冲突展示范围。
3. 讨论有证据且保留歧义的 rename / move+edit 关联，不用启发式匹配取消冲突。
