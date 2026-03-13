/**
 * 应用常量配置
 */

// API 配置
export const API_TIMEOUT = 120000 // 2 分钟超时

// 上传进度配置
export const PROGRESS_BASE = 20
export const PROGRESS_SCALE = 0.6

// PDF 配置
// 优先使用本地 worker，避免 CDN 被墙
export const PDF_WORKER_SRC = import.meta.env.VITE_PDF_WORKER_URL || 
  `${window.location.origin}/pdf.worker.min.mjs`

// 文件上传配置
export const MAX_FILE_SIZE = 10 * 1024 * 1024 // 10MB
export const ACCEPT_IMAGE_TYPES = ['image/jpeg', 'image/png', 'image/gif', 'image/webp', 'image/bmp']
export const ACCEPT_PDF_TYPE = 'application/pdf'

// 聊天配置
export const MAX_CHAT_HISTORY = 50
