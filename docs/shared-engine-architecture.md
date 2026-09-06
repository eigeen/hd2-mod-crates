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

## 2026-09-06：Web 导入内存修复

迁移和修复的 Web 导入入口改为 `PatchFileSources`：读取 TOC，保留 GPU/stream 的 `File` 引用，
按实际文件长度校验，执行任务时才加载 sidecar。旧 `PatchFiles` 缓冲接口保留给执行和兼容调用。
因此选择文件不会提前读取大型 sidecar，但执行阶段仍不是完整的范围读取方案。

TOC 校验、装备识别、图分析和映射预览都在任务专属 Worker 中执行，结束后销毁 Worker。
取消或替换输入会中止相应检查；合并批量导入逐项检查，避免同时创建一批解析任务。
共享 Core 只在来源确实存在多个候选时加载游戏 bundle 索引，普通无歧义识别不访问游戏数据。
两种 Web 合并的执行输入也复用串行读取和取消检查；仍需保留算法所需的完整输入，未宣称执行峰值与文件大小无关。

回归覆盖虚拟 4 GB GPU + 3 GB stream 的导入零读取、截断提前报错、批量顺序执行、取消、
真实 Worker 的反复检查与失败后重新导入。另用仓库 Stylized Dune fixture 的真实 TOC 和
sidecar 文件元数据验证：总输入 158,659,188 字节，仅读取 149,636 字节 TOC，sidecar 内容读取
次数为 0，Worker 成功返回装备图。此验证不代表已复现用户出错文件或完成浏览器 GUI 内存测试。

## 2026-09-06：更新 Mod 的扫描卡顿修复

用用户提供的 Bunny Succubus i-102 Patch 和本机游戏数据复现：旧 Worker/WASM 路径
120 秒仍未完成，已发出 380,829 次读取、累计约 3.19 GB。扫描会完整重建候选归档的 TOC，
并逐个压缩块跨线程读取；界面忽略扫描阶段，只显示固定的 `1/1`。

共享 Core 的 bundled 修复查找现在先读取头与目录表，仅加载匹配的 Unit 正文。
相邻压缩块合并读取，合并上限为 4 MiB（单个更大的块仍可读取）。保持归档扫描顺序、
缺失 Unit 策略和损坏条目的警告行为。Web 同次任务复用 File 快照，不缓存文件内容。
两端共享扫描阶段的翻译与计数显示，并保留取消检查。

同一份 Mod 在 Bun 驱动的真实 Web Worker/WASM 中约 2.1 秒完成：6,299 次读取、
31,011,120 字节，扫描 3,153 个归档，23 个 Unit 均已是当前版本，无删除或警告。
此耗时不包含浏览器文件选择、sidecar 加载与 ZIP 下载，不作为浏览器 GUI 的耗时保证。
另以未改动的同步 BundleIndex 完整重建作基准：原生分段扫描约 1.89 秒，完整扫描约
70.46 秒，输出 TOC 逐字节及结果摘要完全一致。可通过 `repatch_bundle_scan` ignored 测试
配合 `HD2_TEST_DATA_DIR` 与 `HD2_TEST_REPATCH_TOC` 重现；范围读取另有真实归档对照测试。
