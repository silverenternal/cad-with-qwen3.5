import { defineStore } from 'pinia'

/**
 * API 状态管理
 */
export const useApiStore = defineStore('api', {
  state: () => ({
    // API Key 相关
    apiKey: '',
    apiKeySaved: false,
    apiKeyPrefix: '',
    
    // 配额信息
    quotaInfo: null
  }),
  
  getters: {
    /**
     * 是否已登录
     */
    isLoggedIn: (state) => state.apiKeySaved,
    
    /**
     * 获取 API Key 前缀（用于显示）
     */
    displayKey: (state) => state.apiKeyPrefix
  },
  
  actions: {
    /**
     * 保存 API Key
     * @param {string} apiKey - API Key
     */
    saveApiKey(apiKey) {
      if (!apiKey?.trim()) {
        throw new Error('API Key 不能为空')
      }
      
      this.apiKey = apiKey.trim()
      this.apiKeySaved = true
      this.apiKeyPrefix = this.apiKey.substring(0, 8) + '...'
      
      // 持久化到 localStorage
      localStorage.setItem('api_token', this.apiKey)
    },
    
    /**
     * 清除 API Key
     */
    clearApiKey() {
      this.apiKey = ''
      this.apiKeySaved = false
      this.apiKeyPrefix = ''
      this.quotaInfo = null
      
      localStorage.removeItem('api_token')
    },
    
    /**
     * 从 localStorage 加载 API Key
     */
    loadApiKey() {
      const savedKey = localStorage.getItem('api_token')
      if (savedKey) {
        this.apiKey = savedKey
        this.apiKeySaved = true
        this.apiKeyPrefix = savedKey.substring(0, 8) + '...'
        return true
      }
      return false
    },
    
    /**
     * 更新配额信息
     * @param {Object} info - 配额信息
     */
    setQuotaInfo(info) {
      this.quotaInfo = info
    }
  }
})
