# 语言支持

编程语言范围直接复用当前固定版本 weave-core 所用的 sem-core 默认 registry，不另设语言白名单。上游依赖启用 `grammar-all`；本项目与上游解析器使用同一 sem-core 版本。

当前 weave 声明支持的 code grammar 共 34 个、84 个扩展名，包括 Python、Rust、JavaScript、TypeScript、TSX、Go、Java、C、C++、Ruby、C#、PHP、Fortran、Swift、Elixir、Bash、HCL/Terraform、Kotlin、XML、Dart、Perl、SQL、OCaml、OCaml interface、Scala、Zig、Nix、Elm、EDN、Clojure、D、Lua、Fish、BSL。扩展名别名（例如 `.mts`、`.cts`、`.mjs`、`.cjs`）由上游解析器决定。

`tests/languages.rs` 用上游 `supported_merge_extensions()` 与 `get_all_code_extensions()` 的交集枚举覆盖范围。新增 grammar 后若缺少源码 fixture，测试会失败；已有 grammar 新增别名会自动纳入测试。

语言可识别不表示任意输入都能安全分区。语法树错误、实体重叠、同名身份歧义、无法完整还原原文字节等情况仍保守阻断。类和 module 等容器仍按顶层实体处理，不承诺方法粒度。普通 Git 行级冲突始终独立保留。

上游把 Haskell、Vue、Svelte、ERB 列为不推荐 entity merge 的语言。registry 仍可能识别这些后缀，本项目不因识别成功就承诺可靠实体粒度；Haskell fixture 专门记录 binding 可能落入非实体区域的情况。无法可靠分析时保守回退。

JSON、YAML、TOML、CSV、Markdown、LaTeX 等专用数据/文档插件不是当前可靠 code grammar 分析的范围。它们当前未提供可用于本项目语法校验的完整语法树，双方修改时仍可能回退整文件冲突，不把“能提取实体”等同于“已验证解析可靠”。

Git 只会为配置了 `merge=strict-weave` 的路径调用 driver；需在使用它的仓库中为目标语言配置 `.gitattributes`，例如 `*.py merge=strict-weave` 和 `*.rs merge=strict-weave`。prepare 可检查全部变化路径；实际文件 driver 的调用仍由 attributes 决定。
