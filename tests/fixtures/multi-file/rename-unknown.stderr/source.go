冲突 · source.go
feature/ours
base-commit
feature/theirs
原因 [git]：LINE_CONFLICT：Git 行级冲突
原因 [analyze]：ENTITY_ANALYSIS_UNAVAILABLE：无法可靠分析，保留整文件冲突
  依据 [analyze]：原文分区不可用：语言、语法、实体边界或唯一性检查未通过
原因 [analyze]：GLOBAL_MODIFY_VS_MOVE：ƒ calculate 疑似重命名并移动（calculate → renamed），与另一侧变化冲突
  依据 [analyze]：RENAME_MOVE_CANDIDATE：theirs 疑似重命名并移动 ƒ calculate → ƒ renamed · source.go:1 → target.go:1；源 deleted、目标 added；grammar=go、类型相同；按词边界替换各自名称后，区域文本逐 byte 相同（含附着注释）；替换次数=1/1；sources=1，destinations=1；另一侧=无法确认；仅为文本候选，未证明语义等价
