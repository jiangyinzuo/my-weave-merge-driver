冲突 · source.go
ours   : feature/ours
base   : base-commit
theirs : feature/theirs
原因 [git]：LINE_CONFLICT：Git 行级冲突
  依据 [git]：Git 行级合并返回冲突
原因 [weave]：ENTITY_CONFLICT：function calculate 需人工审核
  依据 [weave]：拒绝：modify_delete；modified in theirs
原因 [analyze]：ENTITY_LAYOUT_CHANGED：实体增删或顺序变化
  依据 [analyze]：三方实体或非实体区域序列不同
原因 [analyze]：GLOBAL_MODIFY_VS_MOVE：function calculate 疑似移动与另一侧变化需共同审核
  依据 [analyze]：MOVE_CANDIDATE：ours 疑似移动 function calculate · source.go:1 → target.go:1；源 deleted、目标 added；类型/名称/原始区域文本相同（含附着注释）；sources=1，destinations=1；另一侧=modified
需人工处理：选择一侧 / 编辑合并结果。

