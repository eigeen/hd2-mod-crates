# 更新指南资源

每个版本在 `releases/<id>/` 下维护一组 UTF-8 Markdown 文件：

```text
releases/
└── example-release/
    ├── zh-CN.md
    ├── en.md
    └── preview.webp
```

两个 Markdown 文件必须包含完整的 YAML front matter：

```yaml
---
id: example-release
sequence: 2
version: "0.2.0"
releasedAt: "2026-08-17"
locale: zh-CN
title: 示例更新
---
```

- `id` 必须与目录名相同，发布后不要修改。
- `sequence` 必须是全局唯一、持续递增的正整数；构建时按它排序。
- `id`、`sequence`、`version` 和 `releasedAt` 必须在两个语言文件中一致。
- `locale` 必须与文件名一致；两个语言文件缺一不可。
- 图片使用相对路径，例如 `![预览](./preview.webp)`，支持 PNG、JPEG、WebP 和 GIF。
- 图片不得使用远程 URL，也不能通过 `../` 离开当前版本目录。

`bun run update-info` 会校验全部源文件，只将最近三个版本写入 `public/update-info/`。该目录是构建产物，不应手工编辑或提交。
