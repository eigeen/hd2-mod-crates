# 共享迁移引擎架构

更新日期：2026-09-05。本文件描述 IO/Core 合并后的代码结构。

## 决策：直接合并为 hd2-migrator-core

原 `hd2-migrator-io` 承担了实际运行的归档模型、Unit 算法、四种工具用例、数据读取接口和原生编排。
原 `hd2-migrator-core` 只有一份未被仓库调用的 ordinal remap、报告和 padding 类型；桌面仅声明依赖。
将实际引擎迁到已有的 `hd2-migrator-core` 名称，删除独立 IO crate，比在两包间重新拆分交叉依赖更直接。

以原 IO 的生产实现为准，保留整个模块树及测试。旧 Core 的重复类型和未使用规划函数移除，
不把两套实现简单拼在一起。尤其旧 `build_ordinal_remap` 会无条件把 Unit 标为 skipped，
生产 `build_remap` 则根据数量和后续几何映射处理 Unit；此次不替换生产算法。

仓库内 Web、桌面、CLI、集成测试、Cargo.lock 与 CI 均改用 `hd2-migrator-core`。
不保留旧 crate 的转发空壳；外部源码若直接依赖 `hd2-migrator-io`，需更新包名、路径及 Rust import。
旧 Core 的 `build_ordinal_remap` / `RemapPlan` 公共入口不再存在；报告和 PaddingMode 采用实际引擎类型。
JavaScript WASM 导出与 Tauri command 名称保持原有协议。

## 模块边界

| 层 | 代码位置 | 职责 |
| --- | --- | --- |
| 共享应用 | `migrator_app` | 工具页面、请求组装、任务状态、取消和报告接入 |
| 共享界面 | `migrator_ui` | 组件、类型、语言文案、错误与报告展示 |
| 共享业务入口 | `hd2-migrator-core::operations` | 迁移、Unit 修复、普通合并、头部合并、检查与预览 |
| 共享算法 | `core::migrator::mode_a_common`、`core::unit`、`core::padding`、`core::refs` | 匹配、改写、修复与组装 |
| 运行归档与输出 | `core::archive`、`core::operations::archive_output` | 归档模型、解析、序列化、输出清单与分块访问 |
| 数据访问 | `core::io` | `DataSource` / `IoFuture`、bundle 读取与 `NativeDataSource` |
| 原生编排 | `core::migrator`、`operations::unified_migration::parallel` | CLI 执行与原生并行调度，复用共享计算 |
| Web 适配 | `hd2-migrator-wasm`、`web_ui` | JS 数据源回调、Worker、浏览器文件授权、Blob ZIP 下载 |
| 桌面适配 | `desktop/src-tauri`、`desktop/src` | IPC、游戏目录发现、文件对话框、任务取消、ZIP 提交与更新 |

调用方向：

```text
共享 React 应用
  ├─ Web 适配 → Worker → WASM 桥接 ─┐
  └─ 桌面适配 → Tauri IPC ─────────┴→ core::operations
                                        ↓
                               共享算法 / 归档 / 数据源接口

CLI → core::migrator → 共享计算 + 原生读取与输出
```

`core` 表示共同使用的引擎，不承诺整个 crate 完全没有文件系统代码。
原生读取实现和历史 CLI 编排仍在包内，WASM 宿主使用回调数据源；
已有原生专用并行入口继续使用目标平台 `cfg`，本次没有新增区分业务功能的 Cargo feature。
桌面保存、浏览器下载等宿主副作用仍由包外适配器负责。

## 后续 feature 放置规则

1. 新增或调整业务规则，在 `core::operations` 及其共用算法中实现。
2. 选项、默认值、选择流程和报告在共享前端实现；平台适配器传递已编译请求。
3. 文件授权、原生目录、最终保存、Worker / IPC 和更新只在相应宿主实现。
4. 资源预算和并发可以随平台变化，冲突覆盖规则、输出内容与摘要语义应一致。
5. 优先在现有模块内按职责拆分；只有明确的独立依赖边界和调用需求才新增 crate。

## 尚未包含的整理

- `hd2-archive-format` 与 Core 内运行归档模型仍有重复，需要以兼容 fixture 支撑后续合一。
- Mod 执行输入仍为完整 `PatchBytes`；本次包合并没有引入范围读取输入或降低输入内存峰值。
- 保留 `core::web` 兼容转发和 `Web*` 历史类型名，后续可以独立清理命名。
- `hd2-unit-matching` 等独立包没有因为本次合并而自动成为运行引擎依赖，不能据目录名推断调用关系。
- 完整算法与四种工具的双端行为矩阵仍需继续补齐；改用同一 crate 名称本身不证明行为一致。

## 验证范围

包合并需要验证整个 workspace 的目标编译、共享与宿主 Rust 测试、精确 Clippy、
WASM 实际生成及前端 Worker 测试。外部游戏 fixture 的跳过不计为真实数据兼容验证。

本轮执行结果：

- `cargo check --locked --workspace --all-targets` 通过，包括移入 Core 的集成测试编译。
- Core / WASM / 桌面库测试 201 项通过、4 项 ignored；CLI 测试 5 项通过。
- Core / WASM 的库 Clippy、桌面全目标 Clippy 均在 `-D warnings` 下通过。
- `bun run web:build` 通过，实际重新编译共享 Core 为 WASM 并生成 Worker 产物。
- `bun run ui:test` 117 项通过、2 项依赖外部 fixture 的测试跳过。
- workspace 检查仍有独立 SVD 工具的既有警告，Web 构建仍有主 bundle 体积警告。

日常 CI 同步覆盖新的 Core 包名、CLI 测试及共享消费者的全目标编译；Web 发布路径包含 Core。
未运行真实游戏数据端到端测试，未构建 Tauri 安装包；构建与测试均在本地执行。
