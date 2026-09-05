# 桌面端与 Web 端行为统一重构方案

最新结构：`hd2-migrator-io` 已合并为 `hd2-migrator-core`，详见 [共享迁移引擎架构](shared-engine-architecture.md)。
下文审查与首批实施记录中的 IO 路径是历史名称，当前对应代码位于 Core 目录。

分析基线：2026-09-05，dev / e4d782c。
开篇为实施前的架构审查；首批实施结果见后文「实施进展」，最新包结构见上方链接。

## 目标与判断

两端应具有相同的业务规则、默认选项、交互流程、输入判定、结果语义。
允许差异：操作系统文件访问与浏览器授权、Rust native 与 WASM、资源预算、并发度、数据传输和最终保存方式、桌面自动更新。
资源限制可以阻止某个任务执行，但不应成为隐藏普通功能入口、复制业务实现的理由。

现有 migrator_app 的抽取方向正确，迁移、修复、普通合并、头部合并也都调用 hd2-migrator-io 内的共享 Rust 逻辑。
当前问题是共享边界不完整：页面共用，适配器仍负责导入规则、任务组装、错误分类和输出规则；Rust 的共享用例仍混在 web 与 io 命名空间中。

## 已确认的代码问题

以下路径相对于仓库根目录。行为判断来自代码检查，未通过真实双端 GUI 操作逐项复现。

### 1. 平台能力混入产品规则

- crates/migrator_app/src/platform.ts:44 的 PlatformCapabilities 同时放置 batchTargetSelection、confirmMultiTarget、confirmRepatch、mappingLimits、recommendDesktop。
- crates/web_ui/src/webPlatform.tsx:50 关闭批量选择，并开启两种额外确认；桌面 desktopPlatform.tsx:37 相反。
- MigratorApp.tsx:360 根据平台开关决定是否提供批量选择回调。

批量选择本身并不要求原生能力。即使 Web 有容量限制，也应在共享选择/执行规则中限制数量，而不是维护不同功能。
平台返回资源上限和可用文件能力；共享应用根据任务估算决定是否提示、允许选择和允许执行。桌面推荐属于宿主附加展示，不应参与业务可用性判断。

### 2. 合并导入仍是不同产品流程

- WebManualMergeTool.tsx 与 WebHeadArmorMergeTool.tsx 使用 manualMergeInputs.ts，一次提交可生成多个 Patch 记录，支持 DataTransfer 拖入。
- DesktopSharedManualMergeTool.tsx 与 DesktopSharedHeadMergeTool.tsx 调用 describePatch/inspectPatch，每次返回一个记录，没有 recordsFromDrop。
- 桌面 migration/patch.rs 的 select_toc_path 遇到多个主文件会拒绝；Web 合并按文件名分组。
- 桌面会自动寻找邻接 sidecar，并在 load_patch 时验证大小；Web 头部候选识别先只读取 TOC，合并导入阶段不走迁移入口的 sidecar 校验。
- Web 合并使用后缀白名单和不区分大小写的分组；迁移入口与桌面使用“不是 sidecar 就是候选 TOC”的规则。错误阶段和文件接受范围也不一致。
- 共享接口 recordsFromDrop(DataTransfer) 实际上预设了浏览器的输入模型，导致桌面拖放能力难以接入共享工具。

应将文件发现与 Patch 分组拆开：宿主发现可访问文件，共享层按统一规则分组；Rust 负责基于 TOC 的格式/长度验证。桌面邻接文件发现保留为访问能力，但发现后的校验、候选排序与错误结构统一。一次只允许一个 Patch 的迁移入口和允许多个 Patch 的合并入口，应使用显式的用例策略。

### 3. 任务生命周期与错误协议未统一

- 迁移/修复在 MigratorApp.tsx 内维护 busy、AbortController、报告历史。
- 两种合并工具各自维护另一套 busy、AbortController、toast、summary，未接入同一个报告与错误入口。
- desktopPlatform.tsx 与两个桌面合并包装器重复了三份等待/取消桥接。
- Web 两个合并适配器声明 cancellable: false，WASM 同步计算直接在页面线程调用；仅增加 AbortSignal 不能让同步计算响应用户点击。
- 桌面返回结构化 CommandError 对象；合并工具的 showError/showMergeError 只识别 Error，普通对象会被 String(error) 转成 [object Object]，取消也会走失败提示。
- 桌面普通合并使用 merge.failed，migrator_ui/src/taskError.ts 的 TaskErrorCode 却没有它；Web wasmErrorCode 将 merge_patches 映射为 patch.inspectFailed。
- 合并工具的 busy 不会上报到 MigratorApp 的 busy；桌面更新弹窗收到的 taskBusy 只来自后者。
- 共享导航可在任务期间切换模式；合并工具卸载时没有统一任务归属、取消或持续展示策略。

建议实现共享 TaskController，明确 preparing、running、cancelling、succeeded、cancelled、failed 状态。保存对话框取消也必须有明确结果，不能与执行失败或无结果混用。
所有工具消费同一进度、错误、完成与历史记录协议；桌面 IPC 和 Web Worker 仅负责桥接。
Web Worker 可把同步 WASM 移出页面线程；取消可先采用终止任务专属 Worker、丢弃未提交输出并在下次任务重建的策略。迁移已有异步读文件回调，移动到 Worker 时需验证目录句柄传递或实现文件读取 RPC，避免一次性假定所有浏览器支持相同机制。
原生继续采用协作取消，不要求与 Worker 采用同样中断机制，但取消后的产品状态一致。

### 4. 已共享的业务规则仍会被绕开

- 两个 platform.tsx 各有一份 migrationOutputFilename 和 PATCH_SUFFIX。
- Web 进度标签调用 migrationMappingLabel，显示来源到目标；桌面自己查目标名称。
- 桌面并行任务按完成事件递增位置计数，标签来自选择顺序；在乱序完成时，不能准确代表具体完成项。
- useMigrationWorkspace.ts 已通过 selectTarget 实现选择；MigratorApp.tsx:207 又写一份选择逻辑。
- 后一份在单选时固定返回 [hash]，不会像 selectTarget 一样再次点击取消。migrationMappings.test.ts:41 的测试验证了工具函数，却没有覆盖实际页面调用链。

请求编译、文件名策略、进度展示与状态转换应各有一个共享入口。平台适配器接收编译好的请求，不能再从 equipmentOptions/variants 自行推导业务决策。
进度事件携带 taskId、workItemId、阶段和计数；并行事件可以乱序，展示层按稳定 ID 汇总。

### 5. 流式能力没有贯穿输入到输出

- 桌面 migration/patch.rs 的 load_patch 使用 std::fs::read 整份读取 TOC/GPU/stream，describe_patch 也调用它；识别、预览、执行会重复加载。
- 游戏数据已有 DataSource.read_range；它解决游戏原始数据读取，没有覆盖 Mod 输入的完整生命周期。
- 桌面迁移使用 StreamTocSerializer 直接写 ZIP，临时文件完成后提交，这是应保留的设计。
- 桌面头部合并调用 merge_head_into_armor，先将完整归档序列化为 PatchBytes，然后分块写 ZIP。分块写入不等于避免完整结果驻留内存。
- Web 头部合并反而已经使用 prepare_head_armor_merge + JsArchiveOutputSink 分块输出。
- Web 迁移按 variant 释放结果，但 unified_migration.rs 的 output_files 仍先 serialize 出 TOC/GPU/stream 三个完整缓冲区。
- Web 普通合并通过 Promise.all 读取所有输入，返回完整 PatchBytes；各用例内存策略不一致。
- fileInputs.ts 的 StoreZipBuilder 将块保存在 Blob 列表中。它可减少 WASM 内存驻留，但不能宣称整个浏览器输出不累积。

短期统一为 PreparedArchive/输出清单 + 共享序列化访问接口，适配器只接收文件条目与数据块。
中期引入可按范围读取的 PatchSource/ArchiveSource，不再用 PatchBytes 作为所有输入的强制协议；保留小样本内存实现。
明确 TOC、原始 sidecar、修改后的片段和最终输出的所有权与释放时机。每个任务单独指定内存预算、并发数、块大小和缓存上限。
不要强制所有算法成为纯流式；需要随机访问或改写的数据可以有界缓存。保留现有 ResourceBytes 和 serializer，逐步迁移。

### 6. Rust 的包名与真实边界不一致

- 共享用例实际位于 hd2-migrator-io/src/web；桌面也大量使用 WebUnifiedMigrateOptions、WebProgress 等类型。
- hd2-migrator-io 的 description 仍是 Native filesystem migrator adapter，但实际同时容纳业务算法、归档模型和两端共享用例。
- hd2-migrator-core 目前主要包含 ordinal remap 与报告类型；桌面 Cargo.toml 声明它，但源码未检索到调用。不能直接把它当成已建成的共享内核。
- hd2-archive-format/src/toc.rs 与 hd2-migrator-io/src/archive.rs 各自定义 TocEntry/StreamToc，后者已具有 ResourceBytes 和流式 serializer，属于两套实际模型。
- serial 与 parallel 迁移已经复用 MigrationExecutorCore、compute_prepared_target 和组装逻辑，但顶层验证、plan 编译、调度与报告收尾仍有重复。

先收敛实际运行代码：在现有 io crate 内建立平台中立的 operations、contracts、execution 模块；web 入口可暂时保留转发以减少迁移面。
Serial/Rayon 只决定相同 WorkPlan 如何调度，计划、单项计算、组装和结果解释保持同源。
归档模型以当前实际使用、功能完整的实现为迁移基础，建立兼容测试后逐步合入 format 层，不能用较旧的 format 实现直接覆盖。
core、unit-matching 等现有拆包必须盘点调用关系后决定接入、合并或删除，避免再增加一组没有承担实际职责的新 crate。
mesh/format.rs 为 1434 行，unified_migration.rs 为 1146 行，超过软限制。按解析、计划、组装、输出、测试拆分；新 Rust 模块使用 name.rs + name/。

### 7. 测试与发布链不能保障一致性

- platformBoundary.test.ts 检查导入字符串、入口行数与 Tailwind 扫描范围，只能证明结构约束，不能证明行为相同。
- mode_a_web_parity.rs 已有同步/异步 Mode A 对照，但依赖外部游戏数据，不是完整桌面 IPC 与 WASM 用例对照。
- web_multi_target_game_data.rs 分别运行串行和并行，只检查数量和非空输出，没有直接比较两路内容。
- 两项 WASM 大文件 E2E 默认依环境开关跳过。
- .github/workflows/deploy-web.yml 的 push 和 pull_request paths 未覆盖 migrator_app、migrator_ui、package.json、bun.lock，也漏了部分 Rust 依赖。仅修改共享页面可能不触发 Web 构建/发布。
- Web workflow 主要执行 build；桌面发布执行桌面 crate 的测试和 clippy，未形成双方共享的日常一致性检查。

应建立所有相关改动都会触发的 CI 门禁，与平台发布解耦。发布保留 main/tag 等既有规则，不要求两端使用相同发布方式。

## 建议的职责划分

    migrator_app
      共享页面、选择规则、请求编译、任务状态、报告与错误展示
          |
          +-- 文件访问端口：选择/授权/枚举/读取/拖放事件
          +-- 引擎端口：inspect / preview / migrate / repatch / merge / headMerge
          +-- 输出端口：准备目标 / 接收输出 / 完成提交 / 放弃
          |
      Web 宿主：浏览器文件 API、Worker/WASM、Blob 下载
      Desktop 宿主：原生对话框、Tauri IPC、Rust 工作线程、文件输出
          |
      共享 Rust：用例、计划、算法、验证、输出清单、报告
          |
      数据源、归档访问、序列化、执行调度策略

这是一组有限职责，不是通用插件框架。先在现有 package/module 内落地，不为每个接口新建 crate。
前端业务类型不应长期从视觉组件包 migrator_ui 导出；将业务契约收敛到无 React 依赖的模块，避免 Rust/TypeScript/IPC 各自维护不一致的错误枚举。
若采用类型生成，应覆盖两个传输边界；生成类型不能替代对实际运行返回值的契约验证。

统一输出协议描述文件名、相对路径、条目顺序、空 sidecar 是否存在、内容及摘要；各端可以选用不同 ZIP writer。
完成状态表示引擎产物已交给宿主：桌面可以确认文件已提交，浏览器下载无法确认用户最终落盘，应在内部结果中区分交付方式。
ZIP 容器的时间戳、压缩元数据不应参与跨端字节一致性要求；解包后的文件路径与内容，以及归一化报告必须一致。

## 分阶段实施

### P0：建立基线与修正已知偏差

1. 修复 CI 路径覆盖，为共享代码运行前端检查及关键 Rust 测试。
2. 形成四种操作的输入、默认参数、错误与输出行为矩阵。
3. 修复 selectTarget 绕过，统一文件名/后缀、错误枚举与呈现、桌面取消桥接。
4. 建立小型确定性 fixture 和适配器契约测试，再进行模块移动。
5. 明确批量选择与确认规则：选择能力一致，资源告警由预算驱动。

验收：同一选择事件和请求在两端编译出相同业务参数；四种操作的失败/取消均得到可读且一致的状态，不出现对象字符串。

### P1：统一前端用例与任务

1. 抽出共享输入描述、分组器与导入控制器，原生拖放输出规范化文件引用。
2. 四种工具接入同一个任务控制器、报告入口与错误呈现。
3. 合并进度通过稳定 ID 汇总；全局任务状态覆盖工具切换和桌面更新。
4. 将能力对象限于真实平台能力，将功能规则与资源策略分离。
5. 保留薄宿主包装器，移除其中的业务推导和重复工作流。

验收：一次多 Patch 导入、拖入、重复文件、缺 sidecar、重新选择、任务中切换工具都有一致规则；合并任务也会阻止需要空闲状态的更新动作。

### P2：统一 Rust 用例和输出协议

1. 将 web 下实际用例迁入中立模块，先保留兼容转发。
2. 共用 WorkPlan 构建、校验、报告收尾，串行与 Rayon 保留不同调度器。
3. 各用例返回归档/输出清单，通过相同 serializer 接入 native writer 与 JS chunk sink。
4. 原生头部合并优先复用 prepared 输出，消除额外完整结果序列化。
5. 移动真实归档模型到唯一权威模块，逐步移除旧类型及无效依赖。

验收：同样输入、选项在 serial/Rayon、buffer/chunk sink 下，解包后文件路径与字节相同；摘要、警告与覆盖顺序一致。

### P3：完善资源控制与 Web 取消

1. 引入 Mod 输入范围读取，拆开元数据检查与完整数据加载。
2. Web 使用任务 Worker，接入统一取消协议。
3. 将并发/缓存/输出预算传给执行器；预算不足返回明确的资源错误。
4. 使用真实大文件场景验证峰值内存、重复运行后的资源释放及取消后无残留结果。

验收：资源不足不会悄悄改输出内容或规则；两端业务结果一致，性能和资源占用允许不同。
流式调整可按用例提前实施，但不能把 UI 全量重写与归档模型迁移塞进同一次提交。

## 必要测试矩阵

- 四种操作：成功、输入错误、缺少/截断 sidecar、取消、保存目标取消、输出失败。
- 导入：多 Patch、同名不同批次、大小写、目录含杂项、sidecar 独立提交、相同内容重复选择。
- 迁移：单/多目标、多个来源、singlePatch、noPadding、keep/drop、Unit 转换与冲突覆盖。
- 合并：记录顺序决定覆盖优先级；头部与护甲同包/分包、双体型和不同 LOD。
- 输出：TOC/GPU/stream 逐项内容 hash、条目路径、空 sidecar、摘要及警告，不比较 ZIP 元数据。
- 调度：串行/并行与缓冲/分块的交叉对照；乱序完成事件不影响输出顺序。
- 应用状态：真实组件事件或应用控制器测试，覆盖实际请求出口，避免只测未被页面调用的帮助函数。
- 无外部游戏数据的小 fixture 必须进日常 CI；真实游戏大样本单独运行，显式报告跳过。

## 本次验证与范围

- 已提交原工作区：e4d782c，refactor: share migrator UI and improve streaming head merge。
- feat/head-armor-merge 已快进合并进 dev，当前分支为 dev；未推送远端。
- bun run ui:typecheck：四个前端包全部通过。
- bun run ui:test：86 通过，2 个需环境参数的 WASM E2E 跳过。
- cargo test -p hd2-migrator-io --lib：169 通过。
- 未启动开发服务器，未执行真实 GUI 双端对照、完整发布构建或全部 Rust workspace 测试。
- 本方案中“已确认”指源码能够明确支持的分叉；结果字节是否已经产生差异，需要上述对照测试判定。

## 2026-09-05 实施进展

本节记录上述基线审查之后的本轮实施结果。前文保留的是实施前判断，因此其中的“当前”“仍未”等描述不能当作最新状态；最新状态以本节和 `operation-behavior-matrix.md` 的后续更新为准。

### 已完成

#### 前端共享流程

- 四种操作已接入应用级任务控制器，统一准备、运行、取消中、成功、已取消和失败状态。控制器按提交边界解释取消：未交付结果在取消后丢弃；宿主明确确认已提交的成功结果可优先于晚到取消，并继续记录报告。
- 任务控制器已处理 React StrictMode 的 effect 清理与重新挂载，旧 generation 的完成回调、报告和进度不会写入新任务；任务忙碌时冻结工具路由，避免切页卸载局部输入流程。
- 迁移请求编译、输出名称、错误码与错误展示已收敛到共享入口。普通合并和头部合并接入统一报告历史，结构化桌面错误不再退化为 `[object Object]`。
- 普通合并与头部合并共用文件分类和分组规则。它能区分不同目录下的同名 Patch，也能把直接选择的主文件与 sidecar 归为一组；桌面目录枚举忽略杂项，但显式选择的不支持文件仍给出错误。
- Web 合并导入在描述阶段只读取 TOC 元数据，通过 Rust `patchSidecarRequirements` 和 `File.size` 检查缺失或截断 sidecar，不为校验提前读取大型 sidecar 内容。
- 桌面保存取消、协作取消和任务报告已接入同一应用协议；原生枚举等挂起回调受 generation/取消状态约束，不会把旧任务结果写回当前页面。

#### Web Worker 与输出

- Web 的迁移、普通合并和头部合并已具备任务专属 Worker 边界。取消会终止 Worker，后续任务创建新 Worker，页面线程不再直接承载这些同步 WASM 合并计算。
- Worker 通过受限 RPC 读取游戏数据，任务输入不会因 transferable 优化而破坏可重复使用的页面缓冲区；结构化读取错误、WASM trap、回调异常和取消均有清理测试。
- 新增普通合并流式 ABI `merge_patches_stream` 和前端 `mergePatchesToZip`。Rust 通过 archive sink 按 `TOC`、`.gpu_resources`、`.stream` 顺序输出，Web 用 `StoreZipBuilder` 接收分块并生成 Blob ZIP；旧 `mergePatches` 缓冲 ABI 继续保留。
- 仓库内真实最小普通合并 Worker 测试会同时运行旧缓冲和新分块路径，解包 ZIP 后逐项比较三份文件与摘要，并验证后输入覆盖前输入和空 sidecar 仍存在。该测试只覆盖普通合并的 buffer/chunk 对照，不代表迁移、Unit 修复和头部合并已经完成全套跨端 parity。

#### Rust 与桌面

- 原 `web` 用例实现已迁入平台中立的 `operations` 模块，并保留兼容转发，Web 和桌面开始复用同一套输入、摘要与错误契约。
- 普通合并和头部合并已提供 prepared archive 输出；桌面 writer 与 JS chunk sink 消费同一输出清单，避免桌面头部合并先生成一份完整输出 Patch 再分块写 ZIP。
- 迁移执行的生产收尾和 receiver 已进一步共用，桌面任务桥接集中处理进度、取消、保存目标与完成提交。
- 已补充 archive serializer、普通合并 prepared/buffered、桌面任务与共享前端边界的确定性测试。

#### CI 与发布触发

- 新增独立日常 CI：fresh checkout 先安装 Rust `wasm32` 工具链并生成 WASM，再运行共享及宿主前端 typecheck/tests；另行运行共享 Rust、WASM 契约和桌面库测试与精确 clippy。
- Web 发布 workflow 的路径过滤已覆盖共享前端、相关 Rust crate、根 `package.json` 和 `bun.lock`。`main` 生产发布和 PR 预览逻辑保持原样，release 签名流程未修改。
- CI 仅使用只读源码权限，没有新增 secrets 输出或外部发布步骤。需要本地游戏数据的外部大型 fixture 继续明确显示为 skipped/ignored，不计入日常行为覆盖。

### 验证结果

以下结果由根任务在当前 `dev` 工作区统一验证：

- `bun run ui:typecheck`：通过。
- `bun run ui:test`：117 通过，2 个依赖外部大型 fixture 的测试跳过。其中包含真实 Worker 普通合并的旧缓冲与新分块输出对照；这个数字不能解释为四种操作或完整迁移算法均已完成跨端 parity。
- Rust 联合库测试：201 通过，4 个依赖外部数据的测试 ignored；其中 `hd2-migrator-io` 172 个、`hd2-migrator-wasm` 1 个、桌面库 28 个。
- shared 与 desktop 的精确 clippy：通过。
- `bun run web:build`：通过，实际生成 WASM 与 module Worker 产物。
- `bun run --cwd crates/desktop build`：桌面前端生产构建通过；该命令没有构建 Tauri 安装包。
- 两项生产构建只有现有主 bundle 超过 500 kB 的体积警告，没有构建失败。

未启动开发服务器，未构建完整 Tauri 安装包，未发布，也未发送外部消息。顶级 Worker 的 `da1f812` 变更已以无提交方式集成到当前 `dev` 工作区；本轮全部改动当前均未暂存、未提交、未推送。

### 明确后续

1. 归档模型仍有两套实现。本轮统一了实际用例和输出协议，但没有把 `hd2-archive-format` 与 `hd2-migrator-io` 的归档类型真正合为唯一权威模型。后续应先建立双模型兼容 fixture，再按调用关系迁移，不能直接用功能较少的实现覆盖现有运行模型。
2. Mod 执行输入仍以完整 `PatchBytes` 加载 TOC、GPU 和 stream。描述阶段已避免读取大 sidecar，执行阶段尚未引入范围读取的 `PatchSource/ArchiveSource`；大型 Mod 的输入峰值内存问题仍存在。
3. 当前 serial/Rayon 对照测试覆盖共用的生产收尾和 receiver 行为，没有运行两套完整算法端到端后逐项比较输出。仍需用仓库内小 fixture 对 serial/Rayon 的解包路径、字节、摘要、警告和顺序做完整 E2E。
4. 普通合并已经有真实 Worker 的 buffer/chunk 对照，其余操作仍需补齐同等级的 buffer/chunk 与 Web/desktop 交叉矩阵，特别是迁移的多来源、多目标、Unit 行为和冲突覆盖，以及头部合并的同包/分包、双体型和不同 LOD。
5. 尚未执行真实 Web 与桌面 GUI 对照。需要按 `operation-behavior-matrix.md` 逐项操作四种工具，核对选择、拖放、重新选择、取消、保存取消、报告、下载/落盘和更新阻止行为。
6. 外部大型 fixture 的 skipped/ignored 结果只说明测试入口存在。后续应在具备数据的独立环境运行内存峰值、取消后释放、重复任务和真实游戏数据兼容验证，并单独记录结果。
