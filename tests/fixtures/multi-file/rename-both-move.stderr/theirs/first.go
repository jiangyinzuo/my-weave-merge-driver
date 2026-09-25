冲突 · theirs/first.go
feature/ours
base-commit
feature/theirs
原因 [analyze]：GLOBAL_MODIFY_VS_MOVE：ƒ calculate 疑似重命名并移动（calculate → renamed），与另一侧变化需共同审核
  依据 [analyze]：RENAME_MOVE_CANDIDATE：theirs 疑似重命名并移动 ƒ calculate → ƒ renamed · source.go:1 → theirs/first.go:1；源 deleted、目标 added；grammar=go、类型相同；按词边界替换各自名称后，忽略 token 间空白后语法结构及有序 token 完全相同（9 个 token，保留注释和字面量原文）；原文存在格式差异；替换次数=1/1；sources=1，destinations=2；另一侧=deleted（ours：疑似移动 ƒ calculate · source.go:1 → ƒ calculate · ours/moved.go:1；候选数=1）；仅为文本候选，未证明语义等价
需人工处理：选择一侧 / 编辑合并结果。

