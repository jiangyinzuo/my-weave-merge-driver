# diff3 与 zdiff3：区别及本项目的展示策略

`diff3` 和 `zdiff3` 都使用 `<<<<<<<`、`|||||||`、`=======`、`>>>>>>>` 标记，展示 ours、base、theirs 三份内容。

主要区别是：**zdiff3 会将冲突块首尾处 ours 和 theirs 相同的行移到块外，让冲突更紧凑。** 即使这些行与 base 不同，也可能被移出去。它不会任意抽走冲突块中间所有相同的行。

## 对比示例

base：

```text
start
old value
end
```

ours：

```text
start
shared addition
ours value
end
```

theirs：

```text
start
shared addition
theirs value
end
```

使用 Git 2.53.0，通过 `git merge-file -p --diff3 -L ours -L base -L theirs ours base theirs` 得到：

```text
start
<<<<<<< ours
shared addition
ours value
||||||| base
old value
=======
shared addition
theirs value
>>>>>>> theirs
end
```

将命令中的 `--diff3` 换为 `--zdiff3`，得到：

```text
start
shared addition
<<<<<<< ours
ours value
||||||| base
old value
=======
theirs value
>>>>>>> theirs
end
```

两边共同新增的 `shared addition` 被放到了冲突块外，只保留一份。这个示例中，两种命令都返回冲突；变化的是展示边界。

## 对本项目的影响

本项目要求：同一实体相对 base 被双方修改，就必须人工处理，即使 ours 和 theirs 的结果完全相同。例如双方分别新增文件，却都把同一个计数从 2 改为 5，结果相同仍可能掩盖丢失更新。

因此，必须区分三个职责：

| 职责 | 本项目要求 |
| --- | --- |
| 判定是否冲突 | 执行严格实体规则，保留 Git 行级冲突和 weave-core 明确拒绝合并的结果；不能由展示时的相同行消除来决定 |
| 决定展示范围 | 由我们的 driver 根据实体三方范围控制，必要时扩展到完整函数／方法，处理重叠、顺序及移动 |
| 渲染冲突文本 | 在选定范围内使用三方标记、中文说明和版本身份；紧凑展示不能取消必须人工处理的冲突 |

**选择 diff3 并不自动意味着展示完整函数。** Git 的行级合并不负责识别函数边界；完整实体展示需要我们的 driver 显式选择范围，并保留对应的三方原文。即便采用 diff3 标记，也不能直接把 Git 默认输出当作完整实体展示。

**选择 diff3 也不会让 Git 对相同修改报冲突。** 若将完全相同的两侧内容直接交给普通行级合并，它们可能被自动接受。我们的严格规则必须单独构造和保留这类冲突。Git 跳过 driver 时的补充检查约束见 [requirement.md 第 8 节](requirement.md#8-整个文件内容相同时的需求约束)。

## 建议的展示策略

- **实体规则触发的冲突：** 默认用 diff3 标记展示完整三方实体，保留两侧相同的修改，不采用 zdiff3 的边界压缩。即使同时触发行级冲突，也优先采用完整实体展示。
- **纯行级冲突：** 可以支持 zdiff3，让冲突更紧凑；这是可选展示能力，不改变必须人工处理的结论。
- **范围无法安全扩展：** 保留原行级冲突块，在报告中关联实体；不能为了完整展示而复制、遗漏或重排代码。

这些是本项目的目标行为，不是设置 Git 的 `merge.conflictStyle` 就能自动获得的能力。自定义 driver 必须显式实现或读取相应展示配置；全局分析结果触发 driver 冲突时，也应使用同一套判定与渲染逻辑。

## 当前可选实现

`strict-weave driver ... --zdiff3` 显式开启紧凑行级展示，默认仍采用原有 diff3 / 完整实体展示，不自动跟随 Git 的全局 `merge.conflictStyle`。严格判定使用原始三份文本的 Git diff3/zdiff3 行级结果和实体规则，保留普通行级模式会报的冲突；单次调用的依据见下文。

纯行级冲突可用 Git zdiff3 渲染。对于原本的实体布局整文件回退，首版只在确认 base 原文未变、双方仅在文件末尾新增不同实体及空白、没有其它严格冲突原因时允许紧凑展示。`adjacent.ts` 属于此类；同名实体的双方新增、同一实体双方修改、无法确认的布局变化仍保留完整三方冲突，不承诺全部冲突都能压缩。

测试框架通过可选的 `.output-zdiff3` 单独验证该模式，见 [testing.md](testing.md)。

## 为什么可以只调用一次 Git

核对 Git **v2.35.0**（已有 zdiff3）及 **v2.53.0** 的 `xdiff/xmerge.c` 后，当前实现直接调用一次 `git merge-file --diff3` 或 `--zdiff3`，同时取得文本和冲突状态。普通模式作为独立测试基准保留，不再作为每次合并的前置调用。

所需关系是：在相同文本、相同 diff 参数、没有 `--ours` / `--theirs` / `--union` 的条件下，**普通模式冲突 ⇒ diff3 和 zdiff3 都冲突**。不要求三种模式的冲突数量或输出范围相同，也不依赖反向蕴含。

源码依据：

1. [`builtin/merge-file.c`](https://github.com/git/git/blob/v2.53.0/builtin/merge-file.c) 默认使用 `XDL_MERGE_ZEALOUS_ALNUM`，`favor = 0`；`--diff3` / `--zdiff3` 修改 `xmp.style`。退出码来自 `xdl_merge()` 的冲突计数，超过 127 截断到 127。
2. [`xdl_do_merge()`](https://github.com/git/git/blob/v2.53.0/xdiff/xmerge.c#L500) 对 diff3/zdiff3 将 level 限制到 `XDL_MERGE_EAGER`。初始冲突建立仅区分 `MINIMAL` 与非 `MINIMAL`，因此这三种模式此时使用相同条件，建立相同的冲突节点（`mode == 0`）。
3. 普通模式继续调用 [`xdl_refine_conflicts()`](https://github.com/git/git/blob/v2.53.0/xdiff/xmerge.c#L364)：可能拆分冲突；当 ours/theirs 的冲突区域完全相同时，可将 `mode` 改成 4，从冲突计数中移除。后续 simplification 只合并已有冲突，不会凭空创建第一个冲突。
4. diff3 跳过上述 refinement；[`xdl_refine_zdiff3_conflicts()`](https://github.com/git/git/blob/v2.53.0/xdiff/xmerge.c#L334) 只移动 ours/theirs 边界、缩短长度，不修改 `mode`、不删除冲突节点。即使缩短到空区域，该节点仍是冲突。
5. [`xdl_cleanup_merge()`](https://github.com/git/git/blob/v2.53.0/xdiff/xmerge.c#L80) 统计 `mode == 0` 的节点。因此普通模式最终仍有冲突时，另外两种模式必然也有冲突；diff3 与 zdiff3 的冲突有无一致。

[v2.35.0 的实现](https://github.com/git/git/blob/v2.35.0/xdiff/xmerge.c) 在上述判定和模式处理上相同。这是对已审计实现及当前命令参数的结论，不宣称 Git 未来版本、不同 diff 算法或额外自动解决选项也必然相同。整个仓库的 rename、文件类型和 merge-base 行为不属于 `merge-file` 的保证范围。

验证包含两部分：

- 固定 fixture 的 8×8 对照直接运行普通、diff3、zdiff3 三种 Git 命令，验证上述单向包含关系和 diff3/zdiff3 判定一致；再检查 driver 保留 `LINE_CONFLICT`，防止实体原因掩盖行级漏报。
- 研究时使用本机 Git 2.53.0、固定随机种子 5320，对 4,000 组三方短文本（重复行、空行、增删改）运行三种模式，未发现漏报或 diff3/zdiff3 判定差异。这是补充实验，不是穷举证明，也未发现足以证明三种模式不等价的反例。

这次删除了普通模式预跑和“第二次意外 clean 时整文件冲突”的回退。每次 `line_merge` 只启动一个 Git 子进程；完整 driver 仍可能为实体间隙调用行级合并，weave 内部也有自己的处理，不能表述成整个 driver 仅启动一个子进程。执行错误继续返回错误，不能作为 clean。
