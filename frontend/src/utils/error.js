/**
 * 统一错误处理工具
 */

import toast from './toast'

/**
 * 错误类型枚举
 */
export const ErrorType = {
  NETWORK: 'network',
  AUTH: 'auth',
  API: 'api',
  VALIDATION: 'validation',
  UNKNOWN: 'unknown'
}

/**
 * 获取友好的错误消息
 * @param {Error} error - 错误对象
 * @returns {string} 友好的错误消息
 */
export function getErrorMessage(error) {
  // 网络错误
  if (!error.response) {
    return error.message || '网络连接失败，请检查网络'
  }

  const { status, data } = error.response

  // 认证错误
  if (status === 401) {
    return '登录已过期，请重新登录'
  }

  // 权限不足
  if (status === 403) {
    return '无权访问此资源'
  }

  // 资源不存在
  if (status === 404) {
    return '请求的资源不存在'
  }

  // 服务器错误
  if (status >= 500) {
    return '服务器错误，请稍后重试'
  }

  // API 返回的错误消息
  if (data?.message) {
    return data.message
  }

  if (data?.error) {
    return data.error
  }

  return error.message || '操作失败'
}

/**
 * 处理 API 错误（显示提示 + 可选的额外处理）
 * @param {Error} error - 错误对象
 * @param {Object} options - 配置选项
 * @param {boolean} options.showToast - 是否显示 toast 提示，默认 true
 * @param {boolean} options.clearToken - 401 时是否清除 token，默认 true
 * @param {boolean} options.silent - 是否静默处理（不显示提示，只打印日志），默认 false
 * @param {Function} options.onAuthError - 认证错误时的回调
 */
export function handleApiError(error, options = {}) {
  const {
    showToast = true,
    clearToken = true,
    silent = false,
    onAuthError
  } = options

  const message = getErrorMessage(error)

  // 401 特殊处理
  if (error.response?.status === 401) {
    if (clearToken) {
      localStorage.removeItem('api_token')
    }
    if (onAuthError) {
      onAuthError(error)
    } else {
      // 默认行为：刷新页面
      window.location.reload()
    }
    return
  }

  // 403 静默处理（已在 api.js 拦截器中记录日志）
  if (error.response?.status === 403) {
    if (showToast && !silent) {
      toast.error(message)
    }
    return
  }

  // 显示错误提示
  if (showToast && !silent) {
    toast.error(message)
  }

  // 开发环境下打印详细错误
  if (import.meta.env.DEV) {
    console.error('API Error:', error)
  }
}

export default {
  ErrorType,
  getErrorMessage,
  handleApiError
}
