import axios from 'axios'

import { API_TIMEOUT } from './config/constants'

const API_BASE = '/api/v1'

/**
 * API 客户端
 *
 * @typedef {Object} ApiResponse
 * @property {any} data - 响应数据
 * @property {number} code - 状态码
 * @property {string} message - 消息
 */

// 创建 axios 实例
const apiClient = axios.create({
  baseURL: API_BASE,
  timeout: API_TIMEOUT,
  headers: {
    'Content-Type': 'application/json'
  }
})

// 请求拦截器 - 添加认证
apiClient.interceptors.request.use(config => {
  const token = localStorage.getItem('api_token')
  if (token) {
    config.headers.Authorization = `Bearer ${token}`
  }
  return config
})

// 响应拦截器 - 只处理 403 日志，401 交给 handleApiError 统一处理
apiClient.interceptors.response.use(
  response => response,
  error => {
    // 403 权限错误记录日志
    if (error.response?.status === 403) {
      console.warn('无权访问:', error.config?.url)
    }
    return Promise.reject(error)
  }
)

export default {
  /**
   * 健康检查
   * @returns {Promise<ApiResponse>}
   */
  health() {
    return apiClient.get('/health')
  },

  /**
   * 获取统计信息
   * @returns {Promise<ApiResponse>}
   */
  getStats() {
    return apiClient.get('/stats')
  },

  /**
   * 图纸分析（图片上传）
   * @param {FormData} formData 
   * @param {Object} config - axios 配置（可包含 onUploadProgress）
   * @returns {Promise<ApiResponse>}
   */
  analyzeImage(formData, config = {}) {
    return apiClient.post('/analyze', formData, {
      headers: {
        'Content-Type': 'multipart/form-data'
      },
      ...config
    })
  },

  /**
   * 对话
   * @param {string} message 
   * @param {string[]} images - Base64 图片数组
   * @returns {Promise<ApiResponse>}
   */
  chat(message, images = []) {
    return apiClient.post('/chat', {
      message,
      images
    })
  },

  /**
   * 查询配额
   * @returns {Promise<ApiResponse>}
   */
  getQuota() {
    return apiClient.get('/quota')
  },

  /**
   * 创建 API Key
   * @param {string} name 
   * @param {number} dailyLimit 
   * @returns {Promise<ApiResponse>}
   */
  createApiKey(name, dailyLimit = 100) {
    return apiClient.post('/api-keys', { name, daily_limit: dailyLimit })
  }
}
