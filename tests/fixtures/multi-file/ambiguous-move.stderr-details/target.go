冲突 · target.go
ours   : feature/ours
base   : base-commit
theirs : feature/theirs
原因 [analyze]：GLOBAL_MODIFY_VS_MOVE：function calculate 疑似移动与另一侧变化需共同审核
  依据 [analyze]：MOVE_CANDIDATE：theirs 疑似移动 function calculate · source.go:1 → target.go:1；源 deleted、目标 added；类型/名称/原始区域文本相同（含附着注释）；sources=1，destinations=2；另一侧=modified
需人工处理：选择一侧 / 编辑合并结果。

