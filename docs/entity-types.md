# Entity 类型与展示符号

`entity_type` 是上游 parser 返回的字符串，不是固定 enum。当前项目锁定 `sem-core 0.25.0` 和 `weave-core` revision `a3f501d19601126fefcc40a3ebb764b8d07d39fc`。以下清单描述这组依赖中代码 parser 与内置 parser plugins 已明确产出的类型；它不是对未来上游版本的封闭承诺。

## 代码 parser 类型

代码 parser 会将多种 tree-sitter 节点归一化为以下类型：

`function`、`method`、`class`、`interface`、`protocol`、`init`、`deinit`、`subscript`、`type`、`associatedtype`、`operator`、`enum`、`mixin`、`extension`、`getter`、`setter`、`record`、`struct`、`union`、`impl`、`trait`、`module`、`object`、`val`、`given`、`package`、`export`、`variable`、`constant`、`signature`、`instance`、`pattern`、`fixity`、`binding`、`inherit`、`static`、`module_type`、`exception`、`class_type`、`external`、`decorated_definition`、`constructor`、`field`、`property`、`annotation`、`template`、`table`、`view`、`index`、`schema`、`trigger`、`sequence`、`database`。

专用 extractor 还会产生 JavaScript / TypeScript 的 `test`、`test_suite`、`test_hook`，Clojure 的 `macro`、`multimethod`、`var`，Elixir 的 `macro`、`guard`，以及 EDN map 的 `entry`。这些 extractor 也会使用上面已有的类型，例如 `function`、`module`、`protocol`。

没有通用映射的节点默认保留 grammar 原名。当前 `entity_node_types` 配置中，这些节点包括：

| Grammar | 未进入通用归一化映射的节点 |
| --- | --- |
| TypeScript / TSX | `internal_module` |
| Rust | `macro_definition` |
| PHP | `trait_declaration` |
| Fortran | `program`、`subroutine` |
| HCL | `attribute`、`block` |
| Kotlin | `companion_object`、`object_declaration`、`secondary_constructor` |
| XML | `element` |
| Dart | `class_member` |
| OCaml | `value_definition` |
| Zig | `test_declaration` |
| Elm | `infix_declaration`、`port_annotation`、`value_declaration` |
| D | `alias_declaration`、`anonymous_enum_declaration`、`auto_declaration`、`destructor`、`manifest_constant`、`mixin_template_declaration`、`module_declaration`、`postblit`、`union_declaration`、`unittest_declaration` |

专用分支可能进一步解包或重新分类这些节点，例如 OCaml 的 let binding 会区分 `function` 与 `variable`，不能仅凭 grammar 节点名推定最终 entity type。最终以 parser 返回值为准。

## 其它 parser plugins

固定版本内置 plugins 还会产生这些类型：

- JSON：`chunk`、`object`、`array`、`property`
- YAML：`chunk`、`section`、`property`
- TOML：`section`、`property`
- CSV：`row`
- Markdown：`preamble`、`heading`
- LaTeX：`preamble`、`section`、`command_definition`、`environment`
- Vue：`sfc_block`
- ERB：`template`、`erb_block`、`erb_expression`
- Svelte：`svelte_module`、`svelte_instance_script`、`svelte_module_script`、`svelte_style`、`svelte_fragment`、`svelte_element`、`svelte_snippet`、`svelte_if_block`、`svelte_each_block`、`svelte_key_block`、`svelte_await_block`、`svelte_component`、`svelte_slot_element`、`svelte_head`、`svelte_body`、`svelte_window`、`svelte_document`、`svelte_component_dynamic`、`svelte_element_dynamic`、`svelte_self`、`svelte_fragment_element`、`svelte_boundary`、`svelte_options`、`svelte_title_element`
- 无专用 parser 或无法解析的文件：`chunk`

插件回退不代表本项目能够安全分析该文件；driver 仍执行自己的可靠性检查与保守回退。

## Weave 内部分析类型

weave 的冲突信息还会用到 `file`、`frame`、`entity-order`，分别描述文件、实体间文本及实体顺序。这些是合并分析的分类标签，不是编程语言 parser 抽取的代码类型。

核对来源：`sem-core 0.25.0` 的 `src/parser/plugins/code/{languages,entity_extractor,oxc_extractor}.rs` 与其它 `src/parser/plugins/*.rs`，以及上述 weave revision 的 `crates/weave-core/src/{merge.rs,v2/mod.rs}`。

## 展示规则

只有含义足够直观的 `function` 使用专属符号 `ƒ`。其它类型，包括测试实体，都使用通用 entity 符号 `◇`。映射读取结构化 `entity_type` 字段；未知类型也显示为 `◇`，但原始类型和名称仍保留在 entity identity、分析 JSON 和 `.entities` 测试数据中。展示符号不参与解析、匹配或冲突判定。

## Fixtures

`tests/fixtures/entity-types/` 用多种语言验证上游解析得到的类型和名称。每个 `.entities` 文件由锁定版本的 parser 生成，并由测试逐 byte 比较。它验证类型信息的传递，不表示每一种类型都支持成员级 merge；冲突分析边界仍由当前实现决定。
