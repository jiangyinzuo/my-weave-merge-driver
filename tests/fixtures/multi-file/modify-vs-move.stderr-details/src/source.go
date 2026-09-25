冲突 · src/source.go
feature/ours
base-commit
feature/theirs
原因 [git]：LINE_CONFLICT：Git 行级冲突
  依据 [git]：Git 行级合并返回冲突
原因 [weave]：ENTITY_CONFLICT：ƒ calculate
  依据 [weave]：拒绝：modify_delete；modified in ours
原因 [analyze]：ENTITY_LAYOUT_CHANGED：实体增删或顺序变化
  依据 [analyze]：三方实体或非实体区域序列不同
原因 [analyze]：GLOBAL_MODIFY_VS_MOVE：ƒ calculate 疑似移动与另一侧变化冲突
  依据 [analyze]：MOVE_CANDIDATE：theirs 疑似移动 ƒ calculate · src/source.go:1 → lib/target.go:1；源 deleted、目标 added；类型/名称/原始区域文本相同（含附着注释）；sources=1，destinations=1；另一侧=modified
