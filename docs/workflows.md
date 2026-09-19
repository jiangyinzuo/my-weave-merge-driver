# 预分析与原生 Git 操作

`strict-weave merge/rebase/pull/stash` 把“确定三方快照 → 全局预分析 → 执行原生 Git”连为一步。实现位于 `src/workflow/`，复用 driver 的分析规则，不自行实现 commit、历史重放或 index 冲突处理。`driver` 和显式 `driver prepare` 可独立使用。

预分析返回 `0` 才执行对应合并步骤；返回 `1` 展示审核项并停止；分析错误返回 `129`。原生 Git 启动后保留其退出码。包装命令不会将审核项伪装成已发生的 index conflict，也没有忽略预分析继续的开关。可以修改输入版本后重试；rebase 也可明确跳过步骤或中止。若自行改用原生 Git，便离开了包装命令的全局检查流程。

## 支持范围

| 命令 | 支持 |
| --- | --- |
| `merge [TARGET]` | 单个目标，省略时取当前 upstream；`--ff`、`--ff-only`、`--no-ff`、`--squash`、`--no-commit`、`--commit`、`--edit`、`--no-edit`、`-m`；`--continue/--abort/--quit` |
| `rebase [UPSTREAM] [BRANCH]` | 默认 upstream、可选本地 branch、`--onto`、`--root`、`-i/--interactive`、`-r/--rebase-merges`；`--continue/--skip/--abort/--quit/--edit-todo/--show-current-patch` |
| `pull [REMOTE] [BRANCH]` | 单一远端 ref；`--rebase[=true/false/interactive/merges]`、`--no-rebase`、`--ff-only`、`--no-ff`、`--squash`、`--no-commit` |
| `stash apply/pop [STASH]` | 默认 `stash@{0}`；`--index`；pop 只接受 `stash@{n}` 以校验将删除的条目 |

四种命令均支持 `--explain-reasons` 展开预分析依据。具体参数及互斥关系见各命令 `--help`。没有任意 Git 参数透传；未知参数会报错。暂不包装 cherry-pick，但配置 driver 后仍可使用原生 `git cherry-pick`。

## merge

要求工作区、index 和未跟踪文件均干净，且没有其它进行中的 Git 操作。先固定 HEAD 和目标 commit，只接受唯一 merge base；多 base、虚拟 base、无共同历史、octopus、自定义 strategy/strategy-option 不支持。预分析有审核项时，尚未执行 merge，HEAD、index 内容和工作区不变。

通过后复查 HEAD 与工作区，再执行原生 `git merge --strategy=ort`。默认允许 fast-forward、自动 commit、不打开 editor；使用显式参数改变这些选择。临时清空当前 branch 的 `mergeOptions`，显式设置 ff/squash/commit 模式，不修改仓库配置。`--no-commit` 与原生 Git 一样不能阻止 fast-forward；需要暂停在 merge commit 前请同时传 `--no-ff`。

## rebase

使用 Git 的 interactive merge backend 和 sequencer。普通 rebase 也使用 todo，只是不打开初始编辑器。交互模式先运行用户的 `GIT_SEQUENCE_EDITOR`，再为最终 todo 的每个 pick/reword/edit/squash/fixup/merge 插入检查用的 `exec`。label/reset/break/exec/drop/update-ref/noop 保留原生含义。`--edit-todo` 会隐藏旧检查行、使用本次选择的 editor，并在编辑后重新插入检查。

每一步在执行时取三方版本：

- 普通重放：base = 原 commit 的唯一 parent，ours = 当前重放位置 HEAD，theirs = 原 commit。根 commit 的 base 是空 tree。
- `--rebase-merges` 中的 merge：ours = 当前 HEAD，theirs = 已重建的 label，base = 两者唯一 merge base。只支持一个 merge 目标。把多 parent commit 当作普通 pick 会拒绝。

成功检查的报告成为该步骤的 active 报告，原生 Git 调用 driver 时校验三方指纹。下一步重新分析，不能复用整个序列开始时的报告。保留 merge commit 时，即使原 merge 曾被 Git 行级合并接受，重建它仍须通过严格检查。

发现审核项时停止在该步骤**执行之前**；此前步骤可能已经重放，Git 也可能已经切换分支或 detach HEAD。此时没有该步骤的冲突文件或 index stages。`strict-weave rebase --continue` 重新检查，`--skip` 仅删除被阻止的那一步，`--abort` 由 Git 恢复原分支。`--skip` 无法确认 todo 与被阻止步骤对应时拒绝操作。原生 Git 在真正重放时出现的冲突，仍按 Git 提示编辑、add，再使用包装命令 continue。

为确保每个选中 commit 实际进入检查流程，显式启用 `--force-rebase --reapply-cherry-picks --empty=keep --keep-empty --no-fork-point`。这与 Git 默认跳过已应用 patch、丢弃空 commit 的行为不同。关闭自动 squash、autostash、updateRefs 和缩写 todo；可在交互 todo 中显式安排 squash/fixup/update-ref。`--rebase-merges` 使用 Git 默认的 no-rebase-cousins；暂不接受其它模式值和 apply backend。

继续、跳过、编辑 todo 请使用 `strict-weave rebase`，以重新校验待办列表。用户自定义 `exec` 仍是任意 shell 命令，本工具不会分析其操作；直接使用原生 Git 编辑/继续或手动删除检查行，也不属于包装流程的保证。

## pull

先要求干净且无进行中操作，再执行原生 `git fetch`，固定 FETCH_HEAD，交给上述 merge/rebase 流程。省略 remote/branch 时分别读取当前 branch 的 remote/merge 配置；显式更换 remote 时建议同时传 branch。只 fetch 一个 ref，不递归 submodule，不接受多个 refspec 或带目标更新的 `src:dst`。

rebase 模式优先取命令行，其次 `branch.<name>.rebase`，再取 `pull.rebase`，未设置则 merge。`--ff-only` 优先要求 fast-forward。ff、commit 等选择按此接口参数确定，不继承 `pull.ff`；需要限制 fast-forward 时请显式传 `--ff-only`。

预分析停止时 fetch 已完成，远端跟踪 refs 和 FETCH_HEAD 可能已更新；merge 模式的本地 HEAD 和文件尚未合并。rebase 模式按逐步骤规则停止，后续使用 `strict-weave rebase --continue/--abort`。

## stash

支持已跟踪文件的工作区/index 修改。使用 `git stash create` 采集当前 tracked worktree 的 tree（只生成悬空对象，不创建 stash 条目），并读取 index tree；不修改已跟踪文件内容。拒绝当前未跟踪文件，避免恢复覆盖。`write-tree/stash create` 可能刷新 index 缓存元数据，不承诺 index 文件逐 byte 不变。

以 stash 的第一 parent 为 base，分别检查工作区与 stash、index 与 stash；`--index` 额外检查 stash 的 index parent。stash 的第三 parent 保存的未跟踪文件通过临时 index 合入候选分析 tree，因此移动到未跟踪文件的 function 也能参与检查。调用 Git 前复查 HEAD、index/worktree tree 和 pop selector。原生 Git 仍负责应用 stash、恢复 index，以及仅在成功时删除条目。

## 报告与边界

报告保存在该 worktree 的 Git 目录下 `strict-weave/report-*/analysis.json`；rebase 使用 `strict-weave/rebase-*/report-*/analysis.json`，逐步原子替换 `strict-weave/rebase-*/active.json`。归档报告只读并保留供检查，完成后可手动清理。rebase 的 `rebase.json` 在结束/abort/quit 后清理。`workflow.lock` 防止同一 worktree 中两个包装操作重叠；异常终止可能留下锁，确认没有运行中的操作后再删除。

执行原生操作时临时把 `merge.strict-weave.driver` 指向当前可执行文件，关闭 rerere 和 renormalize；不写永久配置、不安装 hooks。此临时配置使用默认 diff3 和简洁 driver 诊断；已有 driver 配置中的展示选项不会继承。文件 driver 的选择仍由 `.gitattributes` 决定，其它 driver 保持 Git 行为。预分析本身检查全部变化路径，不局限于 attributes。

这补上了已实现规则中 Git 跳过 driver 的部分盲区，包括双方相同修改，但不是语义正确性或所有移动检测的证明。分析输入限制仍见 [全局分析协议](global-analysis.md)。无法构造可靠上下文时停止；filters 转换、文件 rename 等造成报告输入失配时 driver 报错，不静默使用旧报告。mode 冲突由原生 Git 处理。

包装锁不能锁住其它进程运行的原生 Git，也不能约束 hook/editor/自定义 exec 在检查后修改仓库。操作期间应避免并发修改同一 worktree。当前在 Git 2.53.0 上验证；测试方法见 [testing.md](testing.md#真实-git-cli-端到端测试)。
