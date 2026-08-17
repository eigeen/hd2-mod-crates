---
id: v1.0.1
sequence: 2
version: "1.0.1"
releasedAt: "2026-08-17"
locale: zh-CN
title: 1.0.1 修复更新
---

# 1.0.1 修复更新

## 更新 Mod

- 修复输入缺少 sidecar 或 sidecar 为 0 字节时，输出 ZIP 可能缺少 `.gpu_resources` 或 `.stream` 文件的问题。
- 更新后的 Mod 现在始终输出 TOC、`.gpu_resources` 和 `.stream` 三件套。
