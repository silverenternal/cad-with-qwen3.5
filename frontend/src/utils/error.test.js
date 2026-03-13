import { describe, it, expect } from 'vitest'

import { getErrorMessage } from './error'

describe('error.js', () => {
  describe('getErrorMessage', () => {
    it('应该返回网络错误消息', () => {
      const error = { message: 'Network Error', response: null }
      expect(getErrorMessage(error)).toBe('Network Error')
    })

    it('应该返回 401 错误消息', () => {
      const error = { response: { status: 401, data: {} } }
      expect(getErrorMessage(error)).toBe('登录已过期，请重新登录')
    })

    it('应该返回 403 错误消息', () => {
      const error = { response: { status: 403, data: {} } }
      expect(getErrorMessage(error)).toBe('无权访问此资源')
    })

    it('应该返回 404 错误消息', () => {
      const error = { response: { status: 404, data: {} } }
      expect(getErrorMessage(error)).toBe('请求的资源不存在')
    })

    it('应该返回 500 错误消息', () => {
      const error = { response: { status: 500, data: {} } }
      expect(getErrorMessage(error)).toBe('服务器错误，请稍后重试')
    })

    it('应该返回 API 返回的错误消息', () => {
      const error = { response: { status: 400, data: { message: '自定义错误' } } }
      expect(getErrorMessage(error)).toBe('自定义错误')
    })

    it('应该返回 data.error 字段', () => {
      const error = { response: { status: 400, data: { error: '另一个错误' } } }
      expect(getErrorMessage(error)).toBe('另一个错误')
    })

    it('应该返回默认错误消息', () => {
      const error = { response: { status: 400, data: {} } }
      expect(getErrorMessage(error)).toBe('操作失败')
    })
  })
})
