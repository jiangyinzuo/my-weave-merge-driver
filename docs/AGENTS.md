# 文档维护

继承根目录 [AGENTS.md](../AGENTS.md) 的全部项目约束。

文档按 [README 中的索引](../README.md#文档索引) 分工维护：需求与当前能力分开描述；同一规则保留一处详细定义，其它文档引用。`analysis.md` 和 `conflict-reasons.md` 通过 `include_str!` 用作 Rust 文档，修改后须验证 rustdoc。
