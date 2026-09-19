# 非顶层 entity 冲突展示

用例位于 [tests/fixtures/nested/](../tests/fixtures/nested/)，由现有框架自动发现。每组包含 `.base`、`.ours`、`.theirs`、`.output` 和 `.stderr-details`；后两者分别展示写回源码的冲突块和 `--explain-reasons` 的完整诊断。

这些 fixture 记录当前行为，供人工评估展示粒度，不代表已经实现按嵌套方法精确展示。当前分区由顶层 entity 覆盖其内部 entity，因此方法中的修改可能把未修改的兄弟方法一起放进冲突块。

## 用例索引

| 用例（点击查看 output） | ours / theirs 的修改 | 普通 Git | 当前 driver 展示范围 |
| --- | --- | --- | --- |
| [namespace-function.cpp](../tests/fixtures/nested/namespace-function.cpp.output) | namespace 内同一 function，分别修改 x / z | clean | 整个 namespace math，原因名称为 `module math` |
| [namespace-class-method.cpp](../tests/fixtures/nested/namespace-class-method.cpp.output) | namespace → class → 同一方法，分别修改 x / z | clean | 整个 namespace app，原因名称为 `module app` |
| [class-different-methods.cpp](../tests/fixtures/nested/class-different-methods.cpp.output) | 同一 class 的 first / second 方法 | clean | 整个 class Counter，含未修改的 unchanged 方法 |
| [namespace-macro.cpp](../tests/fixtures/nested/namespace-macro.cpp.output) | 同一个 SCALE 宏的乘数分别改成 3 / 4 | conflict | 整个文件，原因包含 Git 行级冲突及 weave 对 module app 的拒绝 |
| [class-method.py](../tests/fixtures/nested/class-method.py.output) | 同一方法，分别修改 x / z | clean | 整个 class Counter，含未修改的方法 |
| [class-different-methods.py](../tests/fixtures/nested/class-different-methods.py.output) | 同一 class 的 first / second 方法 | clean | 整个 class Counter，含未修改的 unchanged 方法 |
| [nested-class-method.py](../tests/fixtures/nested/nested-class-method.py.output) | Outer → Inner → 同一方法，分别修改 x / z | clean | 整个 class Outer |
| [nested-function.py](../tests/fixtures/nested/nested-function.py.output) | outer 内的 score function，分别修改 x / z | clean | 整个 function outer |

表中普通 Git 结果来自对相同输入运行 `git -c merge.conflictStyle=merge merge-file -p OURS BASE THEIRS`。这 8 组 driver 均返回 `1`。除宏用例外，修改行之间留有未改动文本，便于观察严格 entity 规则新增的冲突，而非相邻行造成的 Git 冲突。

C++ 的 namespace 在上游 entity 分类中显示为 `module`。宏用例的 `#define` 在源码位置上位于 namespace 花括号内，但预处理宏没有 C++ namespace 作用域；这个用例观察的是原文所在区域的冲突，不验证宏展开或语义依赖。Python 没有对应的 namespace 声明或 C++ 预处理宏，使用 class、嵌套 class 和嵌套 function 展示层级。

## 人工检查

先并排阅读 `.base`、`.ours`、`.theirs`，再看 `.output` 的边界和 `.stderr-details` 的原因。特别关注：同一 class 中不同方法的独立修改也被视为 class 双方修改；目前原因没有定位到内部方法。

例如运行 namespace 内的类方法用例：

```sh
FIXTURE=nested/namespace-class-method.cpp cargo test --locked --test fixtures -- --nocapture
```

这会分别验证默认输出和详细原因，逐 byte 比对已有预期，不更新 fixture。全部用例通过 `cargo test --locked --test fixtures` 运行。
