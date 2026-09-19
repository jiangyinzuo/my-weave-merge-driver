冲突 · theirs/second.go
ours   : feature/ours
base   : base-commit
theirs : feature/theirs
原因 [analyze]：GLOBAL_MODIFY_VS_MOVE：function calculate 疑似重命名并移动（calculate → renamed），与另一侧变化需共同审核
  依据 [analyze]：RENAME_MOVE_CANDIDATE：theirs 疑似重命名并移动 function calculate → renamed · source.go:1 → theirs/second.go:1；源 deleted、目标 added；grammar=go、类型相同；按词边界替换各自名称后，区域文本逐 byte 相同（含附着注释）；替换次数=1/1；sources=1，destinations=2；另一侧=deleted（ours：疑似移动 function calculate · source.go:1 → function calculate · ours/moved.go:1；候选数=1）；仅为文本候选，未证明语义等价
需人工处理：选择一侧 / 编辑合并结果。

