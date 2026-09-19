# 自生成冲突块与共同文本裁剪

实现集中在 `src/merge/conflict.rs`，对外仍通过 `merge::conflict_box` 调用。实体分区冲突、整文件保守回退、全局分析新增冲突使用同一个渲染函数；调用方决定原因和输入范围，渲染函数不决定 clean/conflict。

默认裁掉 base、ours、theirs 三方逐 byte 相同的前缀行和后缀行，将共同上下文原样放在 marker 外。中间内容依然用带 base 的三方 marker 包装，原因保持原来的 entity 或文件范围。这个过程不分析内部方法、不匹配 diff hunk、不修改 weave-core。

例如三方共同的 class 声明、方法声明和末尾 return 可以放在块外，class 内部修改的 x 到 z 仍留在同一个块内。块内未修改的 y 不被抽走。若 class 声明本身发生变化，它也留在块内。

## 保留冲突与原文

- 只有三方共同文本才可移出；ours/theirs 相同但不同于 base 的修改必须保留在块内。
- 裁剪前缀后再比较后缀，避免重复行导致两个裁剪范围交叉。
- 比较保留换行符的完整行，区分 LF、CRLF 和无末尾换行。前缀必须以换行结束，防止 marker 接在源码同一行上。
- 共同的无换行末行可以作为后缀原样保留；冲突区内无末尾换行的文本仍需补一个换行容纳 marker，这是原渲染器已有的行为。
- 三方完全相同（包括全空）时保留完整冲突块。调用方可能因全局分析或其他外部原因要求审核，渲染器不能将它消除。
- 自生成的每个冲突块始终保留四条 marker；裁剪不删原因、不改退出码，也不将不同实体块合并或重排。

## 与 zdiff3 的关系

这不是自行实现 Git zdiff3。`--zdiff3` 继续只选择 Git 行级合并的展示风格，默认 Git 风格为 diff3。Git 生成的块原样使用；自生成块在两种模式下都做上述三方共同文本裁剪。

Git zdiff3 可以移出仅 ours/theirs 相同、与 base 不同的边界行。我们的裁剪更保守，要求三方相同，因此同内容双方修改也仍然可见。

## 测试

文本 fixture 保存实际三方输入和预期输出；原有 57 份 output 缩短，更新前逐一验证了沿三种 marker 分支还原出的文本与旧 output 相同，原有详细诊断和退出码未变化。nested 用例增加 10 组边界场景，详见 [nested-entity-fixtures.md](nested-entity-fixtures.md)。

`tests/conflict_blocks.rs` 从 fixture 读取 64 组三方组合，分别沿 base/ours/theirs 分支还原完整文本，检查 byte 保真；另外覆盖全相同强制冲突、空内容、插入/删除、重复边界行、CRLF 和无末尾换行。测试中的几行辅助字符串仅用于换行和 marker 边界。
