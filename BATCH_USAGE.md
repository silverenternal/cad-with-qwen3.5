# 批量处理功能使用指南

## 概述

批量处理功能允许您一次性处理多个 CAD 图纸图片，并导出结果为 JSON 或 CSV 格式。

## 快速开始

### 基本用法

```bash
# 处理目录中的所有图片
cargo run --release -- --batch ./cad_images/

# 指定输出文件
cargo run --release -- --batch ./cad_images/ --output results.json
```

### 完整参数说明

| 参数 | 简写 | 说明 | 默认值 |
|------|------|------|--------|
| `--batch` | `-b` | 图片目录路径（必需） | - |
| `--output` | `-o` | 输出文件路径 | `batch_results_YYYYMMDD_HHMMSS.json` |
| `--concurrency` | `-c` | 并发处理数量 | `4` |
| `--type` | `-t` | 图纸类型描述 | `建筑平面图` |
| `--question` | `-q` | 分析问题 | `分析这张图纸，提取关键信息` |
| `--format` | `-f` | 输出格式（json/csv） | `json` |

## 使用示例

### 示例 1：基本批量处理

```bash
cargo run --release -- --batch ./test_batch/
```

输出：
```
批量处理目录：./test_batch/
并发度：4
图纸类型：建筑平面图
问题：分析这张图纸，提取关键信息
输出格式：json
输出路径：batch_results_20260227_100000.json
```

### 示例 2：指定所有参数

```bash
cargo run --release -- \
  --batch ./cad_images/ \
  --output ./results/analysis.json \
  --concurrency 8 \
  --type "结构平面图" \
  --question "提取房间数量和面积信息" \
  --format json
```

### 示例 3：导出 CSV 格式

```bash
cargo run --release -- \
  --batch ./cad_images/ \
  --output results.csv \
  --format csv
```

### 示例 4：自定义问题批量分析

```bash
cargo run --release -- \
  --batch ./cad_images/ \
  --type "建筑平面图" \
  --question "这张图纸中有多少个房间？每个房间的尺寸是多少？" \
  --output room_analysis.json
```

## 输出结果示例

### JSON 格式

```json
{
  "batch_id": "550e8400-e29b-41d4-a716-446655440000",
  "started_at": "2026-02-27T10:00:00Z",
  "completed_at": "2026-02-27T10:05:00Z",
  "total": 10,
  "success": 8,
  "failed": 2,
  "results": [
    {
      "file": "1.jpg",
      "drawing_type": "建筑平面图",
      "question": "分析这张图纸，提取关键信息",
      "status": "success",
      "answer": "这是一张建筑平面图，包含 3 个房间...",
      "latency_ms": 1200
    },
    {
      "file": "2.jpg",
      "drawing_type": "建筑平面图",
      "question": "分析这张图纸，提取关键信息",
      "status": "failed",
      "error": "API 超时"
    }
  ],
  "stats": {
    "avg_latency_ms": 1500,
    "success_rate": 80.0
  }
}
```

### CSV 格式

```csv
file,drawing_type,question,status,answer,error,latency_ms
1.jpg，建筑平面图，分析这张图纸，提取关键信息，success,"这是一张建筑平面图，包含 3 个房间...",,1200
2.jpg，建筑平面图，分析这张图纸，提取关键信息，failed,,,API 超时
```

## 支持的图片格式

- JPG / JPEG
- PNG
- GIF
- WEBP
- BMP

## 并发处理说明

- 默认并发数为 4，可根据系统性能调整
- 较高的并发数可以加快处理速度，但可能增加 API 负载
- 建议根据 API 速率限制调整并发数

## 错误处理

- 单个文件处理失败不会影响其他文件
- 所有结果（包括成功和失败）都会保存在输出文件中
- 失败的文件会包含错误信息说明原因

## 高级技巧

### 1. 批量处理不同图纸类型

如果目录中包含不同类型的图纸，建议分批处理：

```bash
# 处理建筑平面图
cargo run --release -- --batch ./building_plans/ --type "建筑平面图" --output building_results.json

# 处理结构图
cargo run --release -- --batch ./structural_plans/ --type "结构平面图" --output structural_results.json
```

### 2. 自定义问题模板

可以针对不同的分析目的使用不同的问题：

```bash
# 提取房间信息
cargo run --release -- --batch ./plans/ --question "有多少个房间？每个房间的面积是多少？"

# 提取尺寸信息
cargo run --release -- --batch ./plans/ --question "图纸中标注的所有尺寸是什么？"

# 提取材料信息
cargo run --release -- --batch ./plans/ --question "图纸中使用了哪些建筑材料？"
```

### 3. 与 Web API 结合使用

批量处理完成后，可以使用 Web API 查看和管理结果：

```bash
# 启动 Web 服务器
cargo run --release -- --server

# 访问 http://localhost:3000 查看结果
```

## 性能优化建议

1. **调整并发数**：根据 API 响应时间调整并发数
2. **批量大小**：大量文件建议分批处理，避免单次处理时间过长
3. **输出格式**：CSV 格式更适合后续数据处理，JSON 格式更适合程序读取

## 故障排除

### 问题：处理速度慢

- 检查网络连接
- 降低并发数避免 API 限流
- 使用本地 Ollama 模型代替 Cloud 模型

### 问题：内存占用高

- 降低并发数（`--concurrency 2`）
- 分批处理大量文件

### 问题：API 超时

- 检查 API Key 是否有效
- 检查网络连接
- 增加超时时间（需修改配置）

## 技术细节

### 目录结构

```
cad-with-qwen3.5-main/
├── src/
│   ├── batch.rs         # 批量处理核心逻辑
│   ├── batch_result.rs  # 结果数据结构
│   └── main.rs          # 命令行入口
├── test_batch/          # 测试图片目录
└── BATCH_USAGE.md       # 本文档
```

### 核心组件

- **BatchProcessor**: 批量处理器，负责并发控制和结果收集
- **FileResult**: 单个文件的处理结果
- **BatchResult**: 批量处理的汇总结果

## 更新日志

### v0.5.0 (2026-02-27)

- ✅ 新增 `--batch` 参数支持批量处理
- ✅ 支持递归扫描子目录
- ✅ 支持并发处理（可配置）
- ✅ 支持 JSON 和 CSV 输出格式
- ✅ 实时进度显示
- ✅ 详细的统计信息

---

**提示**: 如有问题或建议，请查看项目 README.md 或提交 Issue。
