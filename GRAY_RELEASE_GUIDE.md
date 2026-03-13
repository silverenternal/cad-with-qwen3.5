# 🚀 灰度发布指南

本文档描述如何从 CLI 工具升级到支持灰度发布的 Web API 服务。

---

## 📋 目录

1. [紧急修复 (Phase 0)](#phase-0-紧急修复)
2. [Web API 化 (Phase 1)](#phase-1-web-api-化)
3. [灰度控制 (Phase 2)](#phase-2-灰度控制)
4. [数据收集 (Phase 3)](#phase-3-数据收集)
5. [可观测性 (Phase 4)](#phase-4-可观测性)
6. [红线检查清单](#-红线检查清单)

---

## Phase 0: 紧急修复

### 0.1 撤销 .env 提交历史

**⚠️ 警告：你的 API Key 已暴露，请立即执行以下操作：**

```bash
# 方法 1: 使用 git filter-branch
git filter-branch --force --index-filter \
  "git rm --cached --ignore-unmatch .env" \
  --prune-empty --tag-name-filter cat -- --all

# 方法 2: 使用 BFG Repo-Cleaner (推荐，更快)
# 下载 https://rtyley.github.io/bfg-repo-cleaner/
java -jar bfg.jar --delete-files .env .

# 清理并压缩仓库
git reflog expire --expire=now --all
git gc --prune=now --aggressive

# 强制推送到远程
git push --force --all
git push --force --tags
```

### 0.2 轮换 API Key

1. 访问 Ollama 控制台
2. 撤销当前 API Key
3. 生成新的 API Key
4. 复制 `.env.example` 为 `.env` 并填入新 Key

### 0.3 验证 .gitignore

确保 `.gitignore` 包含：

```
# ============ 敏感信息 ============
.env
.env.local
.env.*.local
*.pem
*.key
secrets/
```

---

## Phase 1: Web API 化

### 1.1 启动 Web 服务器

```bash
# CLI 模式（默认）
cargo run

# Web API 服务器模式
cargo run -- --server
# 或
cargo run -- -s
```

### 1.2 API 端点

| 端点 | 方法 | 描述 | 认证 |
|------|------|------|------|
| `/api/v1/health` | GET | 健康检查 | 不需要 |
| `/api/v1/stats` | GET | 统计信息 | Bearer Token |
| `/api/v1/analyze` | POST | 图纸分析 | Bearer Token |
| `/api/v1/chat` | POST | 对话 | Bearer Token |

### 1.3 使用示例

```bash
# 健康检查
curl http://localhost:3000/api/v1/health

# 图纸分析
curl -X POST http://localhost:3000/api/v1/analyze \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -F "image=@cad_image/test.jpg" \
  -F "drawing_type=建筑平面图" \
  -F "question=请分析这张图纸"
```

### 1.4 配置端口

```bash
# 通过环境变量
export SERVER_PORT=8080
cargo run -- --server

# 或在 .env 中添加
SERVER_PORT=8080
```

---

## Phase 2: 灰度控制

### 2.1 启用灰度发布

编辑 `config.toml`：

```toml
[gray_release]
enabled = true
whitelist = ["user_a", "user_b", "test_user"]
quota_per_user = 100  # 每日请求上限
```

### 2.2 白名单管理

```rust
// 检查用户是否在白名单中
use crate::server::gray_release::{GrayReleaseConfig, is_in_gray_whitelist};

let config = GrayReleaseConfig {
    enabled: true,
    whitelist: HashSet::from(["user1".to_string()]),
    quota_per_user: 100,
};

if is_in_gray_whitelist("user1", &config) {
    println!("允许访问");
} else {
    println!("不在白名单");
}
```

---

## Phase 3: 数据收集

### 3.1 启用 SQLite 存储

```bash
# 编译时启用 SQLite 功能
cargo build --features with-sqlite

# 运行服务器
cargo run --features with-sqlite -- --server
```

### 3.2 数据库配置

在 `.env` 中添加：

```
DATABASE_URL=./data/telemetry.db
```

### 3.3 埋点数据格式

每个遥测事件包含：

```json
{
  "event_type": "api_request",
  "user_id": "anon_1234567890",
  "session_id": "sess_1234567890",
  "endpoint": "/api/v1/analyze",
  "latency_ms": 1523,
  "success": true,
  "model_used": "qwen3.5:397b-cloud",
  "timestamp": "2026-02-26T12:34:56Z"
}
```

### 3.4 查询统计

```rust
use crate::db::{init_database, get_stats};

let pool = init_database("./data/telemetry.db").await?;
let stats = get_stats(&pool).await?;

println!("总请求数：{}", stats.total_requests);
println!("成功请求：{}", stats.successful_requests);
println!("失败请求：{}", stats.failed_requests);
println!("平均延迟：{}ms", stats.avg_latency_ms);
```

---

## Phase 4: 可观测性

### 4.1 日志配置

设置 `RUST_LOG` 环境变量：

```bash
# 开发环境
export RUST_LOG=debug

# 生产环境
export RUST_LOG=info,json

# 运行
cargo run -- --server
```

### 4.2 日志输出示例

```json
{"timestamp":"2026-02-26T12:34:56Z","level":"INFO","message":"Server starting on 0.0.0.0:3000","target":"cad_ocr::server"}
{"timestamp":"2026-02-26T12:35:00Z","level":"INFO","message":"Loaded 2 API keys","target":"cad_ocr::server"}
{"timestamp":"2026-02-26T12:35:10Z","level":"INFO","message":"request: /api/chat | latency: 1523ms | success: true","target":"cad_ocr::telemetry"}
```

### 4.3 遥测指标

系统自动记录以下指标：

- **请求数**: 总请求数、成功数、失败数
- **延迟**: P50, P90, P99
- **错误**: 按错误码分类统计
- **用户**: 活跃用户数、会话数

---

## 🚨 红线检查清单

灰度发布前必须完成以下检查：

### 安全

- [ ] API Key 已轮换且从 git 历史彻底清除
- [ ] `.env` 文件已添加到 `.gitignore`
- [ ] 生产环境使用独立的 API Key
- [ ] 启用 HTTPS（生产环境）

### 质量

- [ ] 核心路径测试覆盖率 ≥ 80%
- [ ] 所有 CI 检查通过（fmt/clippy/test）
- [ ] 代码审查完成

### 性能

- [ ] 压力测试：100 并发下 P99 < 5s
- [ ] 内存使用 < 512MB
- [ ] 无内存泄漏

### 运维

- [ ] 错误告警已配置（钉钉/飞书 webhook）
- [ ] 日志收集正常
- [ ] 回滚方案已验证
- [ ] 监控仪表板就绪

### 灰度

- [ ] 白名单用户已确认
- [ ] 配额限制已设置
- [ ] 灰度开关可动态控制

---

## 📝 版本历史

| 版本 | 日期 | 变更 |
|------|------|------|
| 0.5.0 | 2026-02-26 | 新增 Web API 支持、遥测模块、SQLite 存储 |
| 0.4.0 | 2026-02-25 | CLI 对话模式、图片缓存 |
| 0.3.0 | 2026-02-24 | 多轮对话支持 |

---

## 🔗 相关文档

- [SECURITY_FIX.md](SECURITY_FIX.md) - 安全修复指南
- [.env.example](.env.example) - 环境变量示例
- [config.toml.example](config.toml.example) - 配置示例

---

## 📞 支持

如有问题，请提交 Issue 或联系开发团队。
