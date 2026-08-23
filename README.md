# HD2 Mod Tools

用于处理《HELLDIVERS 2》外观 Mod 的 Rust / TypeScript 工作区，提供桌面端、
Web/WASM 和命令行入口。核心功能包括：

- 将护甲、头盔外观迁移到其他装备；
- 通过“更新 Mod 版本”将旧 Unit Patch 从 `10800437` 更新到 `10800438`；
- 从 `bundles.*.nxa` 读取迁移所需的游戏资源；
- 使用经过人工校验的部件映射，并在必要时进行几何和自定义名称匹配；
- 为多个目标生成可直接安装的独立结果。

工具只读取游戏 `data/` 目录，不会修改游戏文件。

“更新 Mod 版本”只转换 Unit TOC 中的版本号和顶点格式字段。普通装备迁移与手动
Patch 合并不会自动执行这项更新，也不会改写 GPU、stream sidecar 或其他 Unit 数据。

## 项目入口

| 入口 | 路径 | 说明 |
|---|---|---|
| Desktop | `crates/desktop` | 基于 Tauri 的原生桌面应用 |
| Web | `crates/web_ui` | 基于 React 和 WASM 的浏览器版本 |
| CLI | `crates/hd2-migrator-cli` | 面向脚本和批处理的命令行工具 |

Desktop 与 Web 共用 `crates/migrator_ui` 中的组件、主题、映射规则和翻译。
Desktop 在原生 Rust 侧处理大型 Mod 与游戏文件；Web 通过浏览器目录访问 API
读取本地游戏数据。

## 环境要求

- Rust toolchain
- Bun 1.3.13（以根目录 `package.json` 为准）
- 构建 Web/WASM 时需要 `wasm-pack`
- 构建 Desktop 时需要 Tauri 2 的平台依赖

首次使用先安装前端依赖：

```powershell
bun install
```

## 构建

从工作区根目录执行：

```powershell
# 构建全部 Rust crate
bun run rust:build

# 构建 Web/WASM
bun run web:build

# 构建 Desktop 发布版
bun run desktop:build
```

Desktop 发布程序位于 `target/release/hd2_migrator_desktop.exe`。

## CLI

构建命令行工具：

```powershell
cargo build --release -p hd2-migrator-cli
./target/release/hd2-migrator --help
```

使用游戏 `data/` 目录迁移 patch：

```powershell
./target/release/hd2-migrator `
  --patch path/to/mod/9ba626afa44a3aa3.patch_0 `
  --data-dir path/to/Helldivers_2/data `
  --out-dir out
```

无参数运行会依次询问 patch、游戏数据目录、分类、目标和输出目录：

```powershell
./target/release/hd2-migrator
```

常用参数：

| 参数 | 用途 |
|---|---|
| `--target a,b,c` | 只迁移到指定哈希或名称 |
| `--category` | 选择 `archivehashes.json` 中的分类，默认 `Armor` |
| `--source` | 手动指定来源装备哈希 |
| `--armor-mapping-json` | 使用外部护甲部件映射表 |
| `--no-padding` | 禁用空网格补齐 |
| `--experimental-partial-remap` | 允许不完整的 Unit 重映射 |
| `--non-interactive` | 缺少必要参数时直接报错，适合 CI 和脚本 |
| `-v` / `-vv` / `-vvv` | 启用 INFO / DEBUG / TRACE 日志 |

完整参数以 `hd2-migrator --help` 为准。

## 工作区结构

| 路径 | 职责 |
|---|---|
| `crates/hd2-archive-format` | 可在 WASM 中使用的归档格式基础能力 |
| `crates/hd2-migrator-data` | 内嵌索引、装备映射和空网格资源 |
| `crates/hd2-unit-matching` | Unit 部件识别与匹配 |
| `crates/hd2-migrator-core` | 平台无关的迁移规划与核心类型 |
| `crates/hd2-migrator-io` | 原生文件系统、归档读取和迁移编排 |
| `crates/hd2-migrator-wasm` | Web 前端使用的 WASM API |
| `crates/hd2-migrator-cli` | 命令行入口 |
| `crates/migrator_ui` | Desktop / Web 共用界面 |
| `crates/desktop` | Tauri 桌面端 |
| `crates/web_ui` | React Web 前端 |
| `crates/svd-core` | SVD 打包与解包核心能力 |
| `crates/svd-pack` | SVD 打包工具 |
| `crates/svd-export` | SVD 导出工具 |

## 内嵌数据

构建时会将下列数据打包进程序：

- `assets/archivehashes.json`：装备哈希与名称索引；
- `assets/archivehash_overrides.json`：同名装备需要固定使用的 archive ID；
- `assets/armor_mappings.merged.json`：护甲主要 Unit 部件映射；
- `assets/helmet_mappings.json`：头盔 Unit 映射；
- `assets/empty_mesh/{toc,gpu,stream}.bin`：默认空网格模板；
- `assets/bonehash.txt`：Unit 自定义匹配使用的网格组名称哈希。

## Credits

Unit repatching 的处理思路受 Evie / RaidingForPants 制作的
[hd2-repatcher](https://github.com/RaidingForPants/hd2-repatcher) 启发；本项目为独立的
Rust/WASM 实现。

护甲与头盔映射表由 [@大紫](https://space.bilibili.com/263230957) 提供；原始 UI
风格由 [@S1lverAkatsuki](https://github.com/S1lverAkatsuki/) 设计。
