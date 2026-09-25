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

## 当前展示策略

默认使用 Git diff3；`strict-weave merge ... --zdiff3`（以及其它支持的操作命令）显式选择 Git zdiff3，不自动跟随 `merge.conflictStyle`。本项目没有自行实现 zdiff3，严格判定与展示分离：

- **自生成 entity/整文件冲突：** 使用 diff3 结构，只裁剪三方共同前后文，双方相同但不同于 base 的修改仍在块内。详见 [conflict-rendering.md](conflict-rendering.md)。
- **仅有行级原因：** 可以采用 Git zdiff3 输出。
- **entity 追加特例：** 原本因布局变化需要整文件回退，但确认 base 原文未变、双方只在末尾新增不同 entity 及空白且无其它严格原因时，zdiff3 可采用 Git 紧凑输出；[adjacent.ts](../tests/fixtures/layout/adjacent.ts.output-zdiff3) 属于此类。

**展示范围仍由 strict-weave 控制。** diff3 不识别 function 边界，也不会让完全相同的双方修改自动成为冲突。Git 的文本相同判断不能替代严格 entity 规则；Git 跳过 strict-weave 的检查约束见 [需求第 8 节](requirement.md#8-整个文件内容相同时的需求约束)。

fixture 中可选的 `.output-zdiff3` 单独验证该模式，不能替代默认输出断言；两种展示共用预期退出码，见 [testing.md](testing.md)。

## 双方新增行的用例

以下用例均在同一个 `function score` 中只新增行；`.output` 和 `.output-zdiff3` 分别断言两种 strict-weave 展示，`.stderr-details` 断言两种风格的原因。表中 Git 结果来自对相同三份文本直接运行 `git merge-file --diff3/--zdiff3` 的对照，两种风格判定相同。

| 用例（链接为 strict-weave 的 zdiff3 输出） | 双方新增方式 | 原生 Git | strict-weave |
| --- | --- | --- | --- |
| [different-lines.go](../tests/fixtures/insertions/different-lines.go.output-zdiff3) | 同一位置分别新增 ours / theirs | conflict，base 区域为空 | conflict，base 区域为空 |
| [identical-lines.go](../tests/fixtures/insertions/identical-lines.go.output-zdiff3) | 同一位置都新增 shared | clean | conflict，保留双方相同新增行 |
| [common-boundaries.go](../tests/fixtures/insertions/common-boundaries.go.output-zdiff3) | 都新增 start/end，中间分别新增 ours / theirs | conflict；zdiff3 把共同新增边界放在块外 | conflict；start/end 仍在块内，因为 base 没有它们 |
| [different-positions.go](../tests/fixtures/insertions/different-positions.go.output-zdiff3) | 同一 function 的两个不同位置分别新增 | clean | conflict；覆盖两处新增及中间共同文本 |

例如 common-boundaries 的原生 Git zdiff3 局部输出为：

```text
    println("start")
<<<<<<< ours
    println("ours")
||||||| base
=======
    println("theirs")
>>>>>>> theirs
    println("end")
```

strict-weave 的对应部分为（省略 marker 后的标签及原因，完整文本见 fixture）：

```text
<<<<<<< ours
    println("start")
    println("ours")
    println("end")
||||||| base
=======
    println("start")
    println("theirs")
    println("end")
>>>>>>> theirs
```

这四组 strict-weave 的默认与 zdiff3 输出相同：它们都触发严格 entity 原因，使用自生成块及三方共同文本裁剪。`--zdiff3` 不会令这一路径采用 Git 的两侧共同边界裁剪；它仍可影响直接使用 Git 输出的场景，例如已有的 [adjacent.ts](../tests/fixtures/layout/adjacent.ts.output-zdiff3)。

## 为什么可以只调用一次 Git

基于 Git **v2.35.0**（已有 zdiff3）及 **v2.53.0** 的 `xdiff/xmerge.c`，当前实现调用一次 `git merge-file --diff3` 或 `--zdiff3`，同时取得文本和冲突状态。普通模式作为独立测试基准，不作为每次合并的前置调用。

所需关系是：在相同文本、相同 diff 参数、没有 `--ours` / `--theirs` / `--union` 的条件下，**普通模式冲突 ⇒ diff3 和 zdiff3 都冲突**。不要求三种模式的冲突数量或输出范围相同，也不依赖反向蕴含。

源码依据：

1. [`builtin/merge-file.c`](https://github.com/git/git/blob/v2.53.0/builtin/merge-file.c) 默认使用 `XDL_MERGE_ZEALOUS_ALNUM`，`favor = 0`；`--diff3` / `--zdiff3` 修改 `xmp.style`。退出码来自 `xdl_merge()` 的冲突计数，超过 127 截断到 127。
2. [`xdl_do_merge()`](https://github.com/git/git/blob/v2.53.0/xdiff/xmerge.c#L500) 对 diff3/zdiff3 将 level 限制到 `XDL_MERGE_EAGER`。初始冲突建立仅区分 `MINIMAL` 与非 `MINIMAL`，因此这三种模式此时使用相同条件，建立相同的冲突节点（`mode == 0`）。
3. 普通模式继续调用 [`xdl_refine_conflicts()`](https://github.com/git/git/blob/v2.53.0/xdiff/xmerge.c#L364)：可能拆分冲突；当 ours/theirs 的冲突区域完全相同时，可将 `mode` 改成 4，从冲突计数中移除。后续 simplification 只合并已有冲突，不会凭空创建第一个冲突。
4. diff3 跳过上述 refinement；[`xdl_refine_zdiff3_conflicts()`](https://github.com/git/git/blob/v2.53.0/xdiff/xmerge.c#L334) 只移动 ours/theirs 边界、缩短长度，不修改 `mode`、不删除冲突节点。即使缩短到空区域，该节点仍是冲突。
5. [`xdl_cleanup_merge()`](https://github.com/git/git/blob/v2.53.0/xdiff/xmerge.c#L80) 统计 `mode == 0` 的节点。因此普通模式最终仍有冲突时，另外两种模式必然也有冲突；diff3 与 zdiff3 的冲突有无一致。

[v2.35.0 的实现](https://github.com/git/git/blob/v2.35.0/xdiff/xmerge.c) 在上述判定和模式处理上相同。这是对已审计实现及当前命令参数的结论，不宣称 Git 未来版本、不同 diff 算法或额外自动解决选项也必然相同。整个仓库的 rename、文件类型和 merge-base 行为不属于 `merge-file` 的保证范围。

固定 fixture 的 8×8 对照运行普通、diff3、zdiff3 三种 Git 命令，验证单向包含关系和 diff3/zdiff3 判定一致，再检查 strict-weave 保留 `LINE_CONFLICT`。测试入口见 [testing.md](testing.md#其它测试)。

每次 `line_merge` 只启动一个 Git 子进程；完整 strict-weave 还可能为分区调用行级合并，weave 内部也可能执行自己的处理。执行错误返回错误，不能作为 clean。
