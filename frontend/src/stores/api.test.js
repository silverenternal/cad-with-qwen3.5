import { createPinia, setActivePinia } from 'pinia'
import { describe, it, expect, beforeEach } from 'vitest'

import { useApiStore } from './api'

describe('stores/api.js', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    // 清理 localStorage
    localStorage.clear()
  })

  it('应该初始化默认状态', () => {
    const store = useApiStore()
    expect(store.apiKey).toBe('')
    expect(store.apiKeySaved).toBe(false)
    expect(store.apiKeyPrefix).toBe('')
    expect(store.quotaInfo).toBeNull()
  })

  describe('isLoggedIn getter', () => {
    it('应该返回是否已登录', () => {
      const store = useApiStore()
      expect(store.isLoggedIn).toBe(false)

      store.apiKeySaved = true
      expect(store.isLoggedIn).toBe(true)
    })
  })

  describe('displayKey getter', () => {
    it('应该返回 API Key 前缀', () => {
      const store = useApiStore()
      store.apiKeyPrefix = 'sk-abc...'
      expect(store.displayKey).toBe('sk-abc...')
    })
  })

  describe('saveApiKey action', () => {
    it('应该保存 API Key', () => {
      const store = useApiStore()
      store.saveApiKey('sk-test-key-123')

      expect(store.apiKey).toBe('sk-test-key-123')
      expect(store.apiKeySaved).toBe(true)
      expect(store.apiKeyPrefix).toBe('sk-test-...')
      expect(localStorage.getItem('api_token')).toBe('sk-test-key-123')
    })

    it('应该去除空格', () => {
      const store = useApiStore()
      store.saveApiKey('  sk-test-key  ')

      expect(store.apiKey).toBe('sk-test-key')
    })

    it('应该拒绝空 API Key', () => {
      const store = useApiStore()
      expect(() => store.saveApiKey('')).toThrow('API Key 不能为空')
      expect(() => store.saveApiKey('   ')).toThrow('API Key 不能为空')
    })
  })

  describe('clearApiKey action', () => {
    it('应该清除 API Key', () => {
      const store = useApiStore()
      store.apiKey = 'sk-test'
      store.apiKeySaved = true
      store.apiKeyPrefix = 'sk-...'
      store.quotaInfo = { daily_limit: 100 }

      store.clearApiKey()

      expect(store.apiKey).toBe('')
      expect(store.apiKeySaved).toBe(false)
      expect(store.apiKeyPrefix).toBe('')
      expect(store.quotaInfo).toBeNull()
      expect(localStorage.getItem('api_token')).toBeNull()
    })
  })

  describe('loadApiKey action', () => {
    it('应该从 localStorage 加载 API Key', () => {
      localStorage.setItem('api_token', 'sk-saved-key')
      const store = useApiStore()

      const loaded = store.loadApiKey()

      expect(loaded).toBe(true)
      expect(store.apiKey).toBe('sk-saved-key')
      expect(store.apiKeySaved).toBe(true)
      expect(store.apiKeyPrefix).toBe('sk-saved...')
    })

    it('当 localStorage 为空时返回 false', () => {
      const store = useApiStore()
      const loaded = store.loadApiKey()
      expect(loaded).toBe(false)
    })
  })

  describe('setQuotaInfo action', () => {
    it('应该设置配额信息', () => {
      const store = useApiStore()
      const quotaInfo = {
        daily_limit: 100,
        used_today: 10,
        remaining: 90
      }

      store.setQuotaInfo(quotaInfo)

      expect(store.quotaInfo).toEqual(quotaInfo)
    })
  })
})
