# CAD 图纸识别工具

基于 Qwen3.5 多模态大模型的 CAD 图纸智能分析工具，支持 **CLI 交互**、**Web API** 和 **批量处理** 三种模式。

**当前版本：v0.10.0**

---

## 📖 5 分钟快速上手

### 第一步：编译项目

```bash
# 进入项目目录
cd C:\Users\141336\codes\cad-with-qwen3.5-main

# 编译（首次编译需要几分钟）
cargo build --release
```

### 第二步：配置 API Key（二选一）

#### 方式 A：使用云端 API（推荐，准确率高）

1. 获取 API Key：访问 https://ollama.com/connect 登录并复制 API Key
2. 创建配置文件：复制 `.env.example` 为 `.env`
3. 编辑 `.env` 文件，填入你的 API Key：

```bash
# .env 文件内容
OLLAMA_API_KEY=你的 API_KEY_粘贴在这里
```

#### 方式 B：使用本地 Ollama（免费，需自己部署）

```bash
# 1. 安装 Ollama：https://ollama.ai
# 2. 下载模型（约 5GB）
ollama pull llava:7b
# 3. 启动服务
ollama serve
```

### 第三步：运行

```bash
# 启动 CLI 交互模式
cargo run --release
```

**完成！** 现在你可以输入 `@图片路径` 来分析 CAD 图纸了。

---

## 🖥️ 三种运行模式

### 模式 1：CLI 交互模式（默认）

适合单人使用，边聊边分析。

```bash
# 启动
cargo run --release

# 或简写
cargo run
```

**使用示例：**

```
╔═══════════════════════════════════════════════════════════╗
║        CAD 图纸识别 - CLI 交互模式 v0.10.0              ║
╚═══════════════════════════════════════════════════════════╝

👤 你：@cad_image/plan.jpg 分析这个户型有几个房间？
🤖 AI: 根据图纸显示，这个户型共有 3 个卧室、2 个客厅...

👤 你：每个房间的面积是多少？
🤖 AI: 主卧约 15 平方米，次卧约 12 平方米...
```

**常用命令速查表：**

| 命令 | 作用 | 示例 |
|------|------|------|
| `@路径` | 附加图片 | `@plan.jpg` |
| `@PDF 文件` | 附加 PDF（自动分页） | `@drawing.pdf` |
| `help` | 查看帮助 | `help` |
| `clear` | 清空对话 | `clear` |
| `stats` | 查看统计 | `stats` |
| `export` | 导出对话 | `export chat.json` |
| `quit` | 退出 | `quit` |

---

### 模式 2：Web API 服务器模式

适合集成到其他系统，或多人共享服务。

```bash
# 启动服务器
cargo run --release -- --server

# 或简写
cargo run --release -- -s
```

**访问地址：**
- 🌐 前端界面：http://localhost:5173（需先启动前端）
- 📡 API 文档：http://localhost:3000/swagger-ui/
- ❤️ 健康检查：http://localhost:3000/api/v1/health

**前端启动（可选）：**

```bash
cd frontend
npm install
npm run dev
```

**API 调用示例：**

```bash
# 图纸分析
curl -X POST http://localhost:3000/api/v1/analyze \
  -H "Authorization: Bearer 你的 API_KEY" \
  -F "image=@cad_image/plan.jpg" \
  -F "question=分析这张图纸"
```

---

### 模式 3：批量处理模式 ⭐

适合一次性处理大量图纸，支持断点续传。

```bash
# 基本用法
cargo run --release -- --batch ./cad_images/

# 完整参数
cargo run --release -- \
  --batch ./cad_images/ \
  --output results.json \
  --concurrency 4 \
  --question "分析这张图纸并提取关键信息"
```

**参数说明：**

| 参数 | 简写 | 说明 | 默认值 |
|------|------|------|--------|
| `--batch` | `-b` | 图片目录路径（**必需**） | - |
| `--output` | `-o` | 输出文件路径 | `batch_results_时间戳.json` |
| `--concurrency` | `-c` | 并发处理数量 | `4` |
| `--question` | `-q` | 分析问题 | `分析这张图纸并提取关键信息` |
| `--resume` | `-r` | 断点续传（会话 ID） | 无 |

**输出示例：**

```json
{
  "batch_id": "550e8400-e29b-41d4-a716-446655440000",
  "total": 10,
  "success": 8,
  "failed": 2,
  "results": [
    {
      "file": "plan_001.jpg",
      "status": "success",
      "answer": "这是一张建筑平面图...",
      "latency_ms": 1200
    }
  ]
}
```

---

## ⚙️ 配置详解

### 1. 环境变量配置（`.env` 文件）

**位置：** 项目根目录下的 `.env` 文件

**创建方法：**

```bash
# Windows (PowerShell)
copy .env.example .env

# Linux/Mac
cp .env.example .env
```

**配置项说明：**

```bash
# ==================== 核心配置 ====================

# Ollama Cloud API Key（云端模式必需）
# 获取地址：https://ollama.com/connect
OLLAMA_API_KEY=你的 API_KEY_在这里

# 本地 Ollama 服务地址（本地模式使用，默认：http://localhost:11434）
# OLLAMA_LOCAL_URL=http://localhost:11434

# Web 服务端口（服务器模式，默认：3000）
# SERVER_PORT=3000

# ==================== 数据库配置 ====================

# 数据库连接 URL（可选，默认：SQLite ./data/telemetry.db）
# 支持 SQLite: sqlite://./cad_ocr.db
# 支持 PostgreSQL: postgres://user:pass@localhost:5432/dbname
# DATABASE_URL=sqlite://./cad_ocr.db

# ==================== 灰度发布配置 ====================

# 启用灰度发布（默认：true）
GRAY_RELEASE_ENABLED=true

# 白名单用户列表（逗号分隔）
GRAY_RELEASE_WHITELIST=user_abc123,user_def456

# 每用户每日配额限制（默认：100）
GRAY_RELEASE_QUOTA_PER_USER=50

# ==================== 日志配置 ====================

# 日志级别：trace/debug/info/warn/error（默认：info）
RUST_LOG=info

# 运行环境：development/staging/production
APP_ENV=development
```

---

### 2. 程序配置（`config.toml` 文件）

**位置：** 项目根目录下的 `config.toml` 文件

**创建方法：**

```bash
# Windows (PowerShell)
copy config.toml.example config.toml

# Linux/Mac
cp config.toml.example config.toml
```

**配置项说明：**

```toml
# ===== 基本配置（90% 用户只需改这一项） =====

# 批处理并发档位：
#   fast: 2 并发，适合测试或小批量（<50 文件）
#   balanced: 4 并发，推荐默认值，适合中等批量（50-500 文件）
#   aggressive: 8 并发，适合大批量处理（>500 文件）
batch_preset = "balanced"

# 默认本地模型（本地模式使用）
default_local_model = "llava:7b"

# 默认云端模型（云端模式使用）
default_cloud_model = "qwen3.5:397b-cloud"

# 默认图纸类型
default_drawing_type = "建筑平面图"

# 图片最大边长（像素，默认 2048）
max_image_dimension = 2048

# ===== 配额和限流配置 =====

# 每日配额限制（每用户）
quota_daily_limit = 100

# 限流：每秒请求数（每用户）
rate_limit_requests_per_second = 10

# 限流：burst 倍数（1.5 表示允许短时间 burst 到 1.5 倍）
rate_limit_burst_multiplier = 1.5

# ===== PDF 转换配置 =====

[pdf]
# PDF 转换 DPI（72-300，推荐 150）
conversion_dpi = 150
# 是否启用 PDF 转换
enabled = true
# 临时文件目录
temp_dir = "./tmp/pdf_convert"

# ===== 模板自动选择配置 =====

[template_selection]
# 是否启用模板自动选择
enabled = true
# 置信度阈值（0.0-1.0，低于此值需要人工确认）
confidence_threshold = 0.6
# 分类策略："hybrid"（推荐）| "multimodal" | "rule_based"
strategy = "hybrid"
# 分类模型（建议使用小模型降低成本）
model = "llava:7b"
```

---

## 🤖 支持的涵洞模板类型（18 种）

系统会自动识别涵洞图纸类型，无需手动选择。

| 分类 | 模板类型 |
|------|----------|
| **表格类** | 涵洞设置一览表、涵洞工程数量表 |
| **布置图类** | 涵洞布置图、暗涵一般布置图 |
| **钢筋图类** | 2m/3m/4m 孔径箱涵钢筋图 |
| **斜涵类** | 30°斜度 2m/3m/4m 孔径箱涵钢筋图 |
| **细部构造** | 涵身接缝防水图、涵长调整及帽石图、止水带安装图、帽石钢筋图、基础钢筋网平面图/侧面图 |
| **方案图类** | 涵长调整方案图 (一/二/三) |
| **斜布钢筋** | 斜涵斜布钢筋组合图 |

---

## 📦 系统要求

### 必需

- **Rust**: 1.75+ （安装：https://rustup.rs）
- **操作系统**: Windows 10+ / Linux / macOS

### 可选

- **Node.js**: 18+（使用前端界面时需要）
- **poppler-utils**: PDF 转换（分析 PDF 时需要）

**安装 poppler-utils：**

```bash
# Ubuntu/Debian
sudo apt-get install poppler-utils

# macOS
brew install poppler

# Windows
# 下载：https://github.com/oschwartz10612/poppler-windows/releases
# 解压后将 bin 目录添加到 PATH
```

---

## ❓ 常见问题（FAQ）

### Q1: 如何获取 API Key？

**A:** 访问 https://ollama.com/connect 登录账户即可看到 API Key。

### Q2: 本地模式和云端模式有什么区别？

| 对比项 | 本地模式 | 云端模式 |
|--------|----------|----------|
| 费用 | 免费 | 按量计费 |
| 准确率 | 一般（llava:7b） | 高（qwen3.5:397b） |
| 速度 | 取决于本地硬件 | 较快 |
| 网络要求 | 不需要 | 需要 |
| 适合场景 | 测试、学习 | 生产环境 |

### Q3: 批量处理太慢怎么办？

**A:** 尝试以下方法：
1. 增加并发数：`--concurrency 8`
2. 在 `config.toml` 中修改 `batch_preset = "aggressive"`
3. 使用本地模型（无网络延迟）

### Q4: PDF 转换失败怎么办？

**A:** 安装 poppler-utils（见上方"系统要求"）。

### Q5: 如何查看日志？

**A:** 设置环境变量 `RUST_LOG=debug` 后重新运行：

```bash
# Windows (PowerShell)
$env:RUST_LOG="debug"; cargo run --release

# Linux/Mac
RUST_LOG=debug cargo run --release
```

### Q6: 如何导出对话历史？

**A:** 在 CLI 模式中输入 `export 文件名.json`：

```
👤 你：export my_chat.json
✅ 对话已导出到 my_chat.json
```

---

## 🛠️ 开发指南

### 运行测试

```bash
cargo test
```

### 代码格式化

```bash
cargo fmt
```

### 代码检查

```bash
cargo clippy -- -D warnings
```

### 项目结构

```
cad-with-qwen3.5-main/
├── src/
│   ├── cli_main.rs      # CLI 模式入口
│   ├── server_main.rs   # Web API 模式入口
│   ├── batch_main.rs    # 批量处理入口
│   ├── app.rs           # 核心应用逻辑
│   ├── domain/          # 领域层（业务模型）
│   ├── application/     # 应用层（用例协调）
│   ├── infrastructure/  # 基础设施层（DB、API）
│   └── server/          # Web 服务器相关
├── frontend/            # 前端界面（Vue 3）
├── .env.example         # 环境变量示例
├── config.toml.example  # 配置文件示例
└── README.md            # 本文档
```

---

## 📚 更多文档

- [批量处理指南](BATCH_USAGE.md) - 批量处理详细用法和技巧
- [后端功能介绍](BACKEND_FUNCTIONALITY.md) - 完整的 API 端点和逻辑流程
- [灰度发布指南](GRAY_RELEASE_GUIDE.md) - 灰度发布配置和运维指南
- [错误处理规范](ERROR_HANDLING_GUIDE.md) - 错误处理最佳实践

---

## 📝 更新日志

### v0.10.0 (当前版本)

- ✅ 支持 18 种涵洞模板自动识别
- ✅ 批量处理支持断点续传
- ✅ Web API 支持 Swagger 文档
- ✅ 配额管理和速率限制
- ✅ Prometheus 监控指标

---

## 📄 许可证

MIT License

---

## 💡 快速参考卡片

```
╔═══════════════════════════════════════════════════════════╗
║                    快速参考卡片                           ║
╠═══════════════════════════════════════════════════════════╣
║  启动 CLI:     cargo run --release                        ║
║  启动服务器：cargo run --release -- --server              ║
║  批量处理：    cargo run --release -- --batch ./images/   ║
║                                                         ║
║  配置 API Key: 复制 .env.example 为 .env 并填入 Key        ║
║  获取 API Key: https://ollama.com/connect                 ║
║                                                         ║
║  CLI 命令：@图片路径 / help / clear / export / quit      ║
║  API 文档：http://localhost:3000/swagger-ui/              ║
╚═══════════════════════════════════════════════════════════╝
```
