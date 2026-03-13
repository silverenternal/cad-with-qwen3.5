# 后端功能介绍与完整逻辑流程

**版本：** v0.10.0  
**最后更新：** 2026-03-12  
**技术栈：** Rust + Axum + SQLite + Prometheus

---

## 📋 目录

1. [系统架构](#系统架构)
2. [核心功能模块](#核心功能模块)
3. [API 端点详解](#api-端点详解)
4. [完整逻辑流程](#完整逻辑流程)
5. [安全与认证](#安全与认证)
6. [配额与限流](#配额与限流)
7. [监控与遥测](#监控与遥测)
8. [批量处理](#批量处理)
9. [错误处理](#错误处理)
10. [配置管理](#配置管理)

---

## 🏗️ 系统架构

### 分层架构图

```
┌─────────────────────────────────────────────────────────┐
│                    Interfaces 层                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │
│  │   CLI       │  │  HTTP API   │  │   Batch     │     │
│  │  (cli_main) │  │ (server_main)│ │ (batch_main)│     │
│  └─────────────┘  └─────────────┘  └─────────────┘     │
├─────────────────────────────────────────────────────────┤
│                  Application 层                          │
│  ┌─────────────────┐  ┌─────────────────────────────┐   │
│  │ DrawingAnalysis │  │ ApiKeyManagement Service    │   │
│  │   Service       │  │  QuotaCheck Service         │   │
│  │ TemplateClassify│  │                             │   │
│  └─────────────────┘  └─────────────────────────────┘   │
├─────────────────────────────────────────────────────────┤
│                    Domain 层                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │
│  │   Drawing   │  │   User      │  │   ApiKey    │     │
│  │   Model     │  │   Model     │  │   Model     │     │
│  └─────────────┘  └─────────────┘  └─────────────┘     │
│  ┌─────────────────────────────────────────────────┐   │
│  │         Domain Services (业务规则)               │   │
│  │  - Template Selection (18 种涵洞模板)             │   │
│  │  - Authentication & Authorization               │   │
│  │  - Quota Management                             │   │
│  └─────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────┤
│               Infrastructure 层                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │
│  │  ApiClient  │  │  Database   │  │   PDF       │     │
│  │  (Ollama)   │  │  (SQLite)   │  │  Converter  │     │
│  └─────────────┘  └─────────────┘  └─────────────┘     │
│  ┌─────────────────────────────────────────────────┐   │
│  │  Template Classifier (多模态 + 规则混合)          │   │
│  │  Confidence Handler (置信度验证)                 │   │
│  └─────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

### 架构特点

| 层级 | 职责 | 关键模块 |
|------|------|----------|
| **Interfaces** | 用户接口 | CLI、HTTP API、Batch 处理器 |
| **Application** | 用例协调 | 图纸分析、API Key 管理、配额检查 |
| **Domain** | 核心业务 | 图纸模型、用户模型、18 种涵洞模板 |
| **Infrastructure** | 技术实现 | SQLite、Ollama API、PDF 转换 |

---

## 🎯 核心功能模块

### 1. 图纸识别与分析

#### 支持的图纸类型

**通用类型：**
- 装配图 (Assembly)
- 零件图 (Part)
- 原理图 (Schematic)
- 管道图 (Piping)
- 电气图 (Electrical)

**涵洞专用类型（18 种模板）：**

| 分类 | 模板类型 | 标识符 |
|------|----------|--------|
| **表格类** | 涵洞设置一览表 | `culvert_setting_table` |
| | 涵洞工程数量表 | `culvert_quantity_table` |
| **布置图类** | 涵洞布置图 | `culvert_layout` |
| | 暗涵一般布置图 | `dark_culvert_layout` |
| **钢筋图类** | 2m 孔径箱涵钢筋图 | `box_culvert_reinforcement_2m` |
| | 3m 孔径箱涵钢筋图 | `box_culvert_reinforcement_3m` |
| | 4m 孔径箱涵钢筋图 | `box_culvert_reinforcement_4m` |
| **斜涵类** | 30°斜度 2m 孔径 | `skewed_box_culvert_reinforcement_2m` |
| | 30°斜度 3m 孔径 | `skewed_box_culvert_reinforcement_3m` |
| | 30°斜度 4m 孔径 | `skewed_box_culvert_reinforcement_4m` |
| **细部构造** | 涵身接缝防水图 | `joint_waterproofing` |
| | 涵长调整及帽石图 | `culvert_length_adjustment` |
| | 止水带安装图 | `water_stop_installation` |
| | 帽石钢筋图 | `cap_stone_reinforcement` |
| | 基础钢筋网平面图 | `foundation_reinforcement_plan` |
| | 基础钢筋网侧面图 | `foundation_reinforcement_side` |
| **方案图类** | 涵长调整方案图 (一) | `culvert_length_adjustment_1` |
| | 涵长调整方案图 (二) | `culvert_length_adjustment_2` |
| | 涵长调整方案图 (三) | `culvert_length_adjustment_3` |
| **斜布钢筋** | 斜涵斜布钢筋组合图 | `skewed_reinforcement_combination` |

#### 模板选择策略

**两种方式：**

1. **多模态模型自动分类**（推荐，准确率 90%+）
   ```rust
   // 使用 llava/qwen 等多模态模型
   let classifier = MultimodalTemplateClassifier::new(config, api_client);
   let result = classifier.classify(image_data).await?;
   // result.template_type: CulvertType
   // result.confidence_score: f32 (0.0-1.0)
   ```

2. **基于规则匹配**（需要 OCR 文本）
   ```rust
   // 使用关键词匹配
   let selector = RuleBasedTemplateSelector::new();
   let result = selector.classify(ocr_text).await?;
   ```

---

### 2. PDF 多页处理

#### PDF 转换流程

```
用户上传 PDF
    │
    ▼
检测 PDF 文件
    │
    ▼
调用 pdftoppm (poppler-utils)
    │
    ▼
逐页转换为 JPG
    │
    ▼
逐页分析（并发控制：最多 2 页并发）
    │
    ▼
生成汇总报告
    │
    ▼
导出 Markdown 报告
```

#### 关键代码

```rust
// src/app.rs - PDF 处理逻辑
for (page_id, base64_image) in pdf_page_results.iter() {
    // 并发控制（信号量）
    let _permit = semaphore.acquire().await.unwrap();
    
    // 逐页调用 API
    let result = crate::recognition_validator::call_with_validation(
        || async { client.chat(messages).await },
        &retry_config
    ).await;
    
    // 收集结果
    page_results.push((page_id.clone(), result));
}

// 生成汇总报告
if successful_results.len() > 1 {
    let summary_prompt = format!("请根据以下 CAD 图纸的各页分析结果，生成一个整体的汇总报告：\n\n{}", ...);
    let summary_content = client.chat(&summary_prompt).await?;
    export_pdf_report(&page_results, summary_content, &report_path)?;
}
```

---

### 3. 多轮对话

#### 对话管理

```rust
// src/dialog.rs
pub struct DialogManager {
    model: String,
    messages: Vec<Message>,
    max_tokens: usize,
    max_rounds: usize,
}

impl DialogManager {
    // 添加用户消息（支持多图片）
    pub fn add_user_with_images(&mut self, content: String, images: Vec<String>) -> Option<TruncateInfo>
    
    // 添加 AI 助手消息
    pub fn add_assistant(&mut self, content: String)
    
    // 构建 API 请求
    pub fn build_request(&self) -> ChatRequest
}
```

#### 对话历史管理

- **最大轮数：** 30 轮（可配置）
- **自动截断：** 超出时自动移除最早的消息
- **持久化：** CLI 模式支持导出对话历史

---

### 4. 批量处理

#### 批量处理架构

```
┌──────────────────────────────────────────────────────┐
│              BatchProcessor                          │
├──────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │
│  │Concurrency  │  │  Circuit    │  │   Dead      │  │
│  │Controller   │  │  Breaker    │  │  Letter Q   │  │
│  └─────────────┘  └─────────────┘  └─────────────┘  │
├──────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │
│  │  Progress   │  │  Session    │  │  Merger     │  │
│  │  Tracker    │  │  Manager    │  │  (汇总)     │  │
│  └─────────────┘  └─────────────┘  └─────────────┘  │
└──────────────────────────────────────────────────────┘
```

#### 批量处理流程

```
1. 扫描目录
   │
2. 生成处理计划（BatchPlanner）
   │
3. 并发处理（ConcurrencyController）
   │   ├─ 成功 → 结果收集
   │   ├─ 失败 → 死信队列（DeadLetterQueue）
   │   └─ 熔断 → 暂停处理（CircuitBreaker）
   │
4. 进度跟踪（ProgressTracker）
   │
5. 断点续传（SessionManager）
   │
6. 结果汇总（Merger）
   │
7. 导出报告（JSON/CSV/Markdown）
```

#### 配置参数

```toml
# config.toml
[batch]
concurrency = 4              # 并发数
max_retries = 3              # 最大重试次数
circuit_breaker_threshold = 5  # 熔断阈值
dead_letter_queue_size = 100   # 死信队列大小
```

---

## 🔌 API 端点详解

### 公开端点（无需认证）

| 端点 | 方法 | 描述 | 响应示例 |
|------|------|------|----------|
| `/api/v1/health` | GET | 健康检查 | `{"code":200,"data":{"status":"healthy"}}` |
| `/api/v1/metrics` | GET | Prometheus 指标 | `# HELP ...` |

### 受保护端点（需要 API Key）

| 端点 | 方法 | 描述 | 请求体 | 响应示例 |
|------|------|------|--------|----------|
| `/api/v1/analyze` | POST | 图纸分析 | `multipart/form-data` | `{"content":"分析结果","latency_ms":1234}` |
| `/api/v1/chat` | POST | 多轮对话 | `{"message":"...","images":[...]}` | `{"content":"回复","session_id":"uuid"}` |
| `/api/v1/quota` | GET | 查询配额 | - | `{"daily_limit":100,"used_today":10}` |
| `/api/v1/api-keys` | POST | 创建 API Key | `{"name":"user1","daily_limit":100}` | `{"key":"sk_xxx..."}` |
| `/api/v1/stats` | GET | 统计信息 | - | `{"total_requests":1000,"avg_latency_ms":500}` |

### 管理员端点（需要 Admin API Key）

| 端点 | 方法 | 描述 |
|------|------|------|
| `/api/v1/admin/keys` | GET | 列出所有 API Key |
| `/api/v1/admin/keys/generate` | POST | 生成新 API Key |
| `/api/v1/admin/keys/revoke` | POST | 吊销 API Key |
| `/api/v1/admin/users/:id/quota` | GET/PUT | 查询/更新用户配额 |
| `/api/v1/admin/gray-release` | GET/PUT | 灰度发布配置 |
| `/api/v1/admin/stats` | GET | 系统统计 |
| `/api/v1/debug/health` | GET | 详细健康检查 |

---

## 🔄 完整逻辑流程

### 流程 1：Web API 图纸分析

```
用户请求
    │
    ▼
1. 速率限制检查 (Rate Limit Middleware)
   └─ 超出限制 → HTTP 429 Too Many Requests
    │
    ▼
2. API Key 认证 (API Key Auth Middleware)
   ├─ 提取 `Authorization: Bearer <key>` header
   ├─ 验证 Key 有效性（数据库/内存）
   └─ 无效 → HTTP 401 Unauthorized
    │
    ▼
3. 灰度发布检查 (Gray Release Middleware)
   ├─ 检查用户是否在白名单
   └─ 非白名单用户访问灰度功能 → HTTP 403 Forbidden
    │
    ▼
4. 配额检查 (Quota Middleware)
   ├─ 查询用户今日已用配额
   └─ 超出配额 → HTTP 429 Quota Exceeded
    │
    ▼
5. 解析 multipart/form-data
   ├─ 提取 image 文件
   ├─ 提取 question 文本
   └─ 提取 drawing_type（已废弃，保留兼容）
    │
    ▼
6. 安全校验
   ├─ 验证 MIME 类型（infer crate）
   ├─ 验证文件大小（最大 10MB）
   └─ 验证图片内容（validate_image_content）
    │
    ▼
7. 自动模板选择（如果启用）
   ├─ 调用 MultimodalTemplateClassifier
   ├─ 使用多模态模型（llava/qwen）分类
   └─ 返回模板类型和置信度
    │
    ▼
8. 构建 Prompt
   ├─ 加载对应模板的提示词
   ├─ 拼接用户问题
   └─ 构建多模态消息
    │
    ▼
9. 调用 AI 服务（ApiClient）
   ├─ 发送到 Ollama API（本地或云端）
   ├─ 带重试机制（指数退避）
   └─ 超时处理（120 秒）
    │
    ▼
10. 响应处理
    ├─ 解析 AI 响应
    ├─ 记录 Prometheus 指标
    ├─ 记录遥测数据（TelemetryRecorder）
    └─ 返回 JSON 响应
    │
    ▼
11. 配额扣减
    └─ 更新用户今日已用配额
```

### 流程 2：CLI 交互模式

```
启动 CLI
    │
    ▼
1. 加载配置
   ├─ 读取 config.toml
   ├─ 加载 .env 环境变量
   └─ 初始化 ApiClient
    │
    ▼
2. 初始化提示词模板
   ├─ 加载默认模板
   └─ 设置 DialogManager
    │
    ▼
3. 进入主循环
   │
   ├─ 显示提示符 "👤 你："
   │
   ├─ 读取用户输入
   │   │
   │   ├─ 解析内置命令
   │   │   ├─ quit/exit → 退出
   │   │   ├─ clear → 清空对话
   │   │   ├─ help → 显示帮助
   │   │   ├─ stats → 显示统计
   │   │   ├─ export → 导出对话
   │   │   └─ diagnose → PDF 诊断
   │   │
   │   └─ 解析 @图片路径
   │       ├─ 检测 PDF 文件
   │       │   └─ 逐页转换（pdftoppm）
   │       └─ 加载图片（ImageCache）
   │
   ├─ 构建对话消息
   │   ├─ 添加用户消息（支持多图片）
   │   └─ 截断历史（如果超出最大轮数）
   │
   ├─ 调用 API（带验证重试）
   │   ├─ RecognitionValidator
   │   ├─ 置信度检查
   │   └─ 自动重试（最多 2 次）
   │
   ├─ 显示 AI 响应 "🤖 AI: ..."
   │
   ├─ 记录遥测
   │   ├─ 请求延迟
   │   ├─ 成功/失败
   │   └─ 模型名称
   │
   └─ 添加到对话历史
   │
   ▼
4. 优雅关闭
   ├─ 捕获 Ctrl+C
   ├─ 等待进行中的请求完成
   ├─ 刷盘遥测数据
   └─ 退出
```

### 流程 3：批量处理

```
启动批量处理
    │
    ▼
1. 解析命令行参数
   ├─ --batch <目录>
   ├─ --output <文件>
   ├─ --resume <会话 ID>
   └─ --concurrency <数量>
    │
    ▼
2. 扫描图片目录
   ├─ 递归查找所有图片
   ├─ 过滤支持的格式（JPG/PNG/GIF/WebP/BMP/PDF）
   └─ 生成处理计划（BatchPlanner）
    │
    ▼
3. 恢复会话（如果 --resume）
   ├─ 加载会话状态（.batch_progress_<id>.json）
   ├─ 跳过已处理的文件
   └─ 恢复死信队列
    │
    ▼
4. 并发处理
   │
   ├─ 并发控制器（ConcurrencyController）
   │   └─ 限制最大并发数
   │
   ├─ 对每个文件：
   │   │
   │   ├─ 读取文件
   │   ├─ 转换为 Base64
   │   ├─ 调用 API（带重试）
   │   │   ├─ 成功 → 收集结果
   │   │   ├─ 临时错误 → 重试（最多 3 次）
   │   │   └─ 永久错误 → 死信队列
   │   │
   │   └─ 更新进度
   │
   ├─ 熔断器（CircuitBreaker）
   │   └─ 连续失败 5 次 → 暂停 30 秒
   │
   └─ 进度跟踪（ProgressTracker）
       └─ 实时显示进度条（indicatif）
   │
   ▼
5. 结果汇总（Merger）
   ├─ 收集所有成功结果
   ├─ 合并死信队列
   └─ 生成汇总统计
    │
    ▼
6. 导出报告
   ├─ JSON 格式
   ├─ CSV 格式
   └─ Markdown 格式
    │
    ▼
7. 保存会话状态
   └─ 支持断点续传
```

---

## 🔐 安全与认证

### API Key 认证

#### 认证流程

```rust
// src/server/auth.rs
pub async fn api_key_auth(
    State(auth_state): State<Arc<AuthState>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // 1. 提取 Authorization header
    let auth_header = request.headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));
    
    // 2. 验证 API Key
    match auth_header {
        Some(key) => {
            // 验证 Key 有效性
            if auth_state.validate_key(key).await {
                // 设置用户 ID 到请求扩展
                Ok(next.run(request).await)
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        None => Err(StatusCode::UNAUTHORIZED),
    }
}
```

#### API Key 管理

```rust
// 生成 API Key
pub fn generate_api_key() -> String {
    format!("sk_{}", Uuid::new_v4().to_string().replace("-", ""))
}

// 存储 API Key（SHA256 哈希）
pub fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}
```

### 路径安全

#### 防止路径遍历攻击

```rust
// src/security.rs
pub fn sanitize_path(root_dir: &Path, user_path: &str) -> Result<PathBuf, SecurityError> {
    // 1. 规范化路径
    let canonical_root = root_dir.canonicalize()?;
    let full_path = canonical_root.join(user_path);
    
    // 2. 解析符号链接
    let canonical_path = full_path.canonicalize()?;
    
    // 3. 验证是否在 root_dir 内
    if !canonical_path.starts_with(&canonical_root) {
        return Err(SecurityError::PathTraversal);
    }
    
    Ok(canonical_path)
}
```

### 文件上传安全

#### MIME 类型验证

```rust
// src/security.rs
pub fn validate_image_content(
    bytes: &[u8],
    allowed_types: &[AllowedImageType],
) -> Result<AllowedImageType, SecurityError> {
    // 使用 infer crate 检测真实 MIME 类型
    let detected = infer::get(bytes)
        .ok_or(SecurityError::UnknownMimeType)?;
    
    // 验证是否在允许列表中
    allowed_types.iter()
        .find(|t| t.mime_type() == detected.mime_type())
        .copied()
        .ok_or(SecurityError::NotAllowedMimeType)
}
```

---

## 📊 配额与限流

### 配额管理

#### 配额模型

```rust
// src/domain/model/user.rs
pub struct UserQuota {
    pub user_id: String,
    pub daily_limit: u32,      // 每日限额
    pub used_today: u32,       // 今日已用
    pub reset_at: DateTime<Utc>, // 重置时间
}

impl UserQuota {
    pub fn remaining(&self) -> u32 {
        self.daily_limit.saturating_sub(self.used_today)
    }
    
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.reset_at
    }
    
    pub fn reset_if_needed(&mut self) {
        if self.is_expired() {
            self.used_today = 0;
            self.reset_at = Utc::now() + Duration::days(1);
        }
    }
}
```

#### 配额检查中间件

```rust
// src/server/quota.rs
pub async fn quota_middleware(
    State(quota_state): State<Arc<QuotaState>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // 1. 获取用户 ID（从认证中间件）
    let user_id = request.extensions()
        .get::<UserId>()
        .ok_or(StatusCode::UNAUTHORIZED)?
        .clone();
    
    // 2. 检查配额
    if quota_state.check_quota(&user_id).await? {
        // 3. 配额充足，继续处理
        let response = next.run(request).await;
        
        // 4. 扣减配额（响应后）
        quota_state.decrement_quota(&user_id).await?;
        
        Ok(response)
    } else {
        // 5. 配额不足，返回 429
        Err(StatusCode::TOO_MANY_REQUESTS)
    }
}
```

### 速率限制

#### 令牌桶算法

```rust
// src/server/rate_limit.rs
use governor::{Quota, RateLimiter};
use nonzero_ext::nonzero;

pub struct RateLimitState {
    limiter: RateLimiter<NotKeyed, InMemoryState>,
}

impl RateLimitState {
    pub fn new(requests_per_second: u32, burst_multiplier: f32) -> Self {
        let burst = nonzero!((requests_per_second as f32 * burst_multiplier) as u32);
        let quota = Quota::per_second(nonzero!(requests_per_second))
            .allow_burst(burst);
        
        Self {
            limiter: RateLimiter::direct(quota),
        }
    }
}

pub async fn rate_limit_middleware(
    State(state): State<Arc<RateLimitState>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if state.limiter.check().is_ok() {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::TOO_MANY_REQUESTS)
    }
}
```

---

## 📈 监控与遥测

### Prometheus 指标

#### 暴露的指标

```
# 请求指标
http_requests_total{method, path, status}
http_request_duration_seconds{method, path}
http_requests_in_flight

# 业务指标
cad_analysis_total{status, model}
cad_analysis_duration_seconds{model}
cad_pdf_pages_processed_total

# 配额指标
quota_remaining{user_id}
quota_exceeded_total{user_id}

# 错误指标
errors_total{type, endpoint}
```

#### 指标采集

```rust
// src/metrics.rs
pub struct Metrics {
    requests_total: Counter,
    request_duration: Histogram,
    errors_total: Counter,
}

impl Metrics {
    pub fn record_request(&self, duration_secs: f64) {
        self.requests_total.inc();
        self.request_duration.observe(duration_secs);
    }
    
    pub fn record_error(&self) {
        self.errors_total.inc();
    }
}

pub static GLOBAL_METRICS: Lazy<Metrics> = Lazy::new(|| Metrics::new());
```

### 遥测记录

#### 遥测事件

```rust
// src/telemetry.rs
pub enum TelemetryEvent {
    Request {
        endpoint: String,
        latency_ms: u64,
        success: bool,
        model: String,
    },
    Error {
        error_type: String,
        message: String,
        context: serde_json::Value,
    },
}

pub struct TelemetryRecorder {
    user_id: Option<String>,
    db: Option<DbPool>,
    wal: Vec<TelemetryEvent>,  // Write-Ahead Log
}

impl TelemetryRecorder {
    pub async fn log_request(
        &self,
        endpoint: &str,
        latency_ms: u64,
        success: bool,
        model: Option<&str>,
    ) {
        let event = TelemetryEvent::Request {
            endpoint: endpoint.to_string(),
            latency_ms,
            success,
            model: model.unwrap_or("unknown").to_string(),
        };
        
        // 写入 WAL（防止数据丢失）
        self.wal.push(event);
        
        // 异步写入数据库
        if let Some(db) = &self.db {
            let _ = db.log_telemetry(&event).await;
        }
    }
}
```

---

## 📦 批量处理

### 批量处理架构

#### 核心组件

| 组件 | 职责 | 关键方法 |
|------|------|----------|
| **BatchProcessor** | 主处理器 | `process_batch()` |
| **BatchPlanner** | 生成处理计划 | `plan()` |
| **ConcurrencyController** | 并发控制 | `acquire_permit()` |
| **CircuitBreaker** | 熔断器 | `check_state()` |
| **DeadLetterQueue** | 死信队列 | `push_failed()` |
| **ProgressTracker** | 进度跟踪 | `update_progress()` |
| **SessionManager** | 会话管理 | `save_session()` |
| **Merger** | 结果汇总 | `merge_results()` |

#### 配置选项

```toml
# config.toml
[batch]
concurrency = 4                    # 并发数
max_retries = 3                    # 最大重试次数
circuit_breaker_threshold = 5      # 熔断阈值（连续失败次数）
circuit_breaker_timeout_secs = 30  # 熔断超时
dead_letter_queue_size = 100       # 死信队列大小
enable_resume = true               # 启用断点续传
```

### 批量处理流程详解

#### 1. 扫描与规划

```rust
// src/batch/planner.rs
pub struct BatchPlanner {
    input_dir: PathBuf,
    supported_extensions: Vec<String>,
}

impl BatchPlanner {
    pub fn plan(&self) -> Result<BatchPlan> {
        let mut files = Vec::new();
        
        // 递归扫描目录
        for entry in walkdir::WalkDir::new(&self.input_dir) {
            let entry = entry?;
            let path = entry.path();
            
            // 检查文件扩展名
            if let Some(ext) = path.extension() {
                if self.supported_extensions.contains(&ext.to_lowercase()) {
                    files.push(path.to_path_buf());
                }
            }
        }
        
        Ok(BatchPlan { files, total: files.len() })
    }
}
```

#### 2. 并发处理

```rust
// src/batch/concurrency_controller.rs
use tokio::sync::Semaphore;

pub struct ConcurrencyController {
    semaphore: Arc<Semaphore>,
}

impl ConcurrencyController {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }
    
    pub async fn acquire_permit(&self) -> SemaphorePermit {
        self.semaphore.acquire().await.unwrap()
    }
}
```

#### 3. 熔断器

```rust
// src/batch/circuit_breaker.rs
pub struct CircuitBreaker {
    failure_count: AtomicUsize,
    state: AtomicU8,  // 0=Closed, 1=Open, 2=HalfOpen
    threshold: usize,
    timeout: Duration,
}

impl CircuitBreaker {
    pub fn check_state(&self) -> CircuitState {
        match self.state.load(Ordering::SeqCst) {
            0 => CircuitState::Closed,
            1 => {
                // 检查超时
                if self.timeout_elapsed() {
                    self.state.store(2, Ordering::SeqCst);
                    CircuitState::HalfOpen
                } else {
                    CircuitState::Open
                }
            }
            2 => CircuitState::HalfOpen,
            _ => unreachable!(),
        }
    }
    
    pub fn record_success(&self) {
        self.failure_count.store(0, Ordering::SeqCst);
        self.state.store(0, Ordering::SeqCst);
    }
    
    pub fn record_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        if count >= self.threshold {
            self.state.store(1, Ordering::SeqCst);
        }
    }
}
```

#### 4. 死信队列

```rust
// src/batch/dead_letter_queue.rs
pub struct DeadLetterQueue {
    queue: Arc<Mutex<VecDeque<FailedItem>>>,
    max_size: usize,
}

impl DeadLetterQueue {
    pub fn push_failed(&self, file: PathBuf, error: String, retries: u32) {
        let mut queue = self.queue.lock().unwrap();
        
        // 超出大小时移除最早的
        if queue.len() >= self.max_size {
            queue.pop_front();
        }
        
        queue.push_back(FailedItem {
            file,
            error,
            retries,
            timestamp: Utc::now(),
        });
    }
    
    pub fn get_all(&self) -> Vec<FailedItem> {
        self.queue.lock().unwrap().clone().into_iter().collect()
    }
}
```

---

## ❌ 错误处理

### 错误类型

```rust
// src/error.rs
#[derive(Debug, Error)]
pub enum Error {
    /// IO 错误
    #[error("IO 错误：{0}")]
    Io(#[from] std::io::Error),
    
    /// API 错误
    #[error("API 错误：{0}")]
    Api(#[from] ApiError),
    
    /// 领域错误
    #[error("领域错误：{0}")]
    Domain(#[from] DomainError),
    
    /// 认证错误
    #[error("认证失败：API Key 无效或已过期")]
    Authentication,
    
    /// 配额错误
    #[error("配额已用尽")]
    QuotaExceeded,
    
    /// 速率限制
    #[error("请求过于频繁")]
    RateLimited,
    
    /// 文件错误
    #[error("文件错误：{path} - {message}")]
    FileError {
        path: String,
        message: String,
    },
    
    /// PDF 转换错误
    #[error("PDF 转换失败：{0}")]
    PdfConversion(String),
}
```

### 错误处理策略

#### 1. 可重试错误

```rust
// src/infrastructure/external/mod.rs
impl ApiError {
    pub fn is_retryable(&self) -> bool {
        match self {
            // 网络错误通常可重试
            ApiError::NetworkError(e) => {
                e.is_timeout() || e.is_connect() || e.is_request()
            }
            // 服务端 5xx 错误可重试
            ApiError::ServerError { status, .. } => status.is_server_error(),
            // 速率限制可重试（带退避）
            ApiError::RateLimitExceeded => true,
            // 以下错误不应重试
            ApiError::InvalidApiKey => false,
            ApiError::ModelNotFound { .. } => false,
            ApiError::ClientError { .. } => false,
            ApiError::JsonError(_) => false,
            _ => false,
        }
    }
}
```

#### 2. 重试机制

```rust
// src/infrastructure/external/mod.rs
pub async fn chat(&self, messages: &[Message]) -> Result<String, ApiError> {
    let url = format!("{}/api/chat", self.base_url);
    let body = serde_json::json!({ "model": self.model, "messages": messages });
    
    let mut last_error = None;
    for attempt in 0..=self.max_retries {
        if attempt > 0 {
            // 指数退避：100ms, 200ms, 400ms, ...
            let delay = Duration::from_millis(100 * (1 << attempt));
            warn!("请求失败，{}ms 后重试 ({}/{})", delay.as_millis(), attempt + 1, self.max_retries + 1);
            tokio::time::sleep(delay).await;
        }
        
        match self.do_request(&url, &body).await {
            Ok(content) => return Ok(content),
            Err(e) => {
                if e.is_retryable() {
                    last_error = Some(e);
                    continue;
                } else {
                    // 不可重试错误，直接返回
                    error!("不可重试的错误：{}", e);
                    return Err(e);
                }
            }
        }
    }
    
    Err(last_error.unwrap())
}
```

#### 3. 置信度验证

```rust
// src/recognition_validator.rs
pub struct RetryConfig {
    pub max_retries: u32,
    pub min_confidence: f32,
    pub enable_validation: bool,
    pub initial_delay_ms: u64,
    pub backoff_multiplier: f32,
    pub max_delay_ms: u64,
    pub weights: ValidationWeights,
}

pub async fn call_with_validation<F, Fut, T>(
    call_fn: F,
    config: &RetryConfig,
) -> Result<T, Error>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, Error>>,
{
    let mut attempt = 0;
    let mut delay_ms = config.initial_delay_ms;
    
    loop {
        let result = call_fn().await?;
        
        // 如果启用验证，检查置信度
        if config.enable_validation {
            let confidence = extract_confidence(&result)?;
            if confidence >= config.min_confidence {
                return Ok(result);
            }
        } else {
            return Ok(result);
        }
        
        // 超出最大重试次数
        attempt += 1;
        if attempt >= config.max_retries {
            return Err(Error::ValidationFailed("置信度不足".to_string()));
        }
        
        // 延迟后重试
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        delay_ms = (delay_ms as f32 * config.backoff_multiplier) as u64;
        delay_ms = delay_ms.min(config.max_delay_ms);
    }
}
```

---

## ⚙️ 配置管理

### 配置文件

#### config.toml 示例

```toml
# 模型配置
default_local_model = "llava:7b"
default_cloud_model = "qwen3.5:397b-cloud"

# 默认图纸类型（已废弃，保留兼容）
default_drawing_type = "建筑平面图"

# 缓存配置
cache_max_entries = 100
cache_ttl_seconds = 3600
max_image_dimension = 2048
jpeg_quality = 85

# 配额管理
quota_daily_limit = 100
quota_fallback_policy = "reject"  # "reject" 或 "memory"

# 速率限制
rate_limit_requests_per_second = 10
rate_limit_burst_multiplier = 2.0

# 批量处理
[batch]
concurrency = 4
max_retries = 3
circuit_breaker_threshold = 5
dead_letter_queue_size = 100

# 模板选择
[template_selection]
enabled = true
classifier_type = "multimodal"  # "multimodal" 或 "rule_based"
min_confidence = 0.7

# 数据库
database_url = "sqlite://cad_ocr.db"

# 日志
log_level = "info"
log_format = "json"  # "json" 或 "text"
```

### 环境变量

```bash
# .env
# API Key（必选）
OLLAMA_API_KEY=your_api_key_here

# 服务器端口（可选，默认 3000）
SERVER_PORT=3000

# 灰度发布配置
GRAY_RELEASE_ENABLED=true
GRAY_RELEASE_WHITELIST=user1,user2,user3
GRAY_RELEASE_QUOTA_PER_USER=1000

# 数据库 URL（可选）
DATABASE_URL=sqlite://cad_ocr.db

# 日志级别
RUST_LOG=info
```

### 配置加载

```rust
// src/config.rs
pub struct Config {
    pub default_local_model: String,
    pub default_cloud_model: String,
    pub quota_daily_limit: u32,
    pub rate_limit_requests_per_second: u32,
    // ...
}

impl Config {
    pub fn load() -> Result<Self> {
        // 1. 加载 config.toml
        let config_file = ConfigBuilder::default()
            .add_source(File::with_name("config").required(false))
            .build()?;
        
        // 2. 加载环境变量（覆盖配置文件）
        let config_env = config_file
            .try_deserialize::<Config>()?;
        
        // 3. 验证配置
        config_env.validate()?;
        
        Ok(config_env)
    }
}
```

---

## 📝 总结

### 核心优势

1. **分层架构清晰** - Domain/Application/Infrastructure 职责分明
2. **安全性完善** - API Key 认证、路径安全、MIME 验证
3. **配额限流健全** - 令牌桶算法、数据库持久化
4. **监控遥测完备** - Prometheus 指标、WAL 日志
5. **批量处理强大** - 并发控制、熔断器、断点续传
6. **错误处理规范** - 可重试/不可重试错误分类

### 适用场景

- ✅ CAD 图纸识别与分析
- ✅ 涵洞图纸自动分类（18 种模板）
- ✅ PDF 多页批量处理
- ✅ 多轮对话交互
- ✅ 企业级配额管理
- ✅ 灰度发布

### 系统要求

| 组件 | 要求 |
|------|------|
| **Rust** | 1.75+ |
| **Ollama** | 本地部署或云端 API |
| **poppler-utils** | PDF 转换（可选） |
| **SQLite** | 配额和遥测存储（可选） |

---

**文档版本：** v1.0  
**维护者：** 开发团队  
**最后更新：** 2026-03-12
