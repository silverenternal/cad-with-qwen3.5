# CAD OCR 前端

基于 Vue 3 的 CAD 图纸识别 Web 前端。

## 快速开始

### 1. 安装依赖

```bash
cd frontend
npm install
```

### 2. 启动后端服务器

在启动前端之前，确保后端服务器正在运行：

```bash
# 在项目根目录
cargo run --release -- --server
```

### 3. 启动前端开发服务器

```bash
npm run dev
```

访问 http://localhost:5173

## 功能

- 📁 **文件上传** - 支持拖拽上传，多文件选择
- 🖼️ **图片分析** - 上传 CAD 图纸进行 AI 分析
- 💬 **智能对话** - 与 AI 进行多轮对话，深入了解图纸细节
- 📊 **结果展示** - 美观的结果展示和复制功能
- 🎨 **现代 UI** - 渐变色设计，响应式布局

## 技术栈

- Vue 3 (Composition API)
- Vite 5
- Axios
- 原生 CSS

## 构建

```bash
# 生产构建
npm run build

# 预览构建结果
npm run preview
```

## API 配置

前端通过 Vite 代理连接到后端 API：
- 开发环境：`http://localhost:3000`
- 代理配置在 `vite.config.js`

## 注意事项

1. 确保后端服务器已启动
2. 首次使用需要配置 API Key（通过后端配置）
3. 支持的文件格式：JPG, PNG, GIF, WebP, BMP, PDF
