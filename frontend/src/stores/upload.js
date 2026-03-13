import { defineStore } from 'pinia'

import { MAX_CHAT_HISTORY } from '@/config/constants'

/**
 * 上传状态管理
 */
export const useUploadStore = defineStore('upload', {
  state: () => ({
    // 选中的文件
    selectedFiles: [],
    
    // 加载状态
    loading: false,
    loadingText: '',
    progress: 0,
    
    // 分析结果
    result: null,
    
    // Base64 图片（用于对话）
    images: []
  }),
  
  getters: {
    /**
     * 是否有选中的文件
     */
    hasFiles: (state) => state.selectedFiles.length > 0,
    
    /**
     * 是否正在加载
     */
    isLoading: (state) => state.loading
  },
  
  actions: {
    /**
     * 设置选中的文件
     * @param {File[]} files - 文件列表
     */
    setFiles(files) {
      this.selectedFiles = files
    },
    
    /**
     * 清除选中的文件
     */
    clearFiles() {
      this.selectedFiles = []
    },
    
    /**
     * 设置加载状态
     * @param {boolean} loading - 是否加载中
     * @param {string} text - 加载提示文本
     * @param {number} progress - 进度（0-100）
     */
    setLoading(loading, text = '', progress = 0) {
      this.loading = loading
      this.loadingText = text
      this.progress = progress
    },
    
    /**
     * 设置分析结果
     * @param {string} result - 分析结果
     */
    setResult(result) {
      this.result = result
    },
    
    /**
     * 清除分析结果
     */
    clearResult() {
      this.result = null
    },
    
    /**
     * 设置 Base64 图片
     * @param {string[]} images - Base64 图片数组
     */
    setImages(images) {
      this.images = images
    },
    
    /**
     * 重置上传状态（用于开始新的分析）
     */
    reset() {
      this.loading = false
      this.loadingText = ''
      this.progress = 0
      this.result = null
      this.images = []
    }
  }
})

/**
 * 聊天状态管理
 */
export const useChatStore = defineStore('chat', {
  state: () => ({
    // 聊天记录
    history: [],
    
    // 当前输入的消息
    message: ''
  }),
  
  getters: {
    /**
     * 是否有聊天记录
     */
    hasHistory: (state) => state.history.length > 0
  },
  
  actions: {
    /**
     * 添加消息到历史记录
     * @param {string} message - 消息内容
     * @param {'user' | 'assistant'} role - 角色
     */
    addMessage(message, role) {
      this.history.push({
        message,
        role,
        timestamp: Date.now()
      })
      
      // 限制历史记录数量
      if (this.history.length > MAX_CHAT_HISTORY) {
        this.history = this.history.slice(-MAX_CHAT_HISTORY)
      }
      
      // 保存到 localStorage
      this.saveToStorage()
    },
    
    /**
     * 清空聊天记录
     */
    clearHistory() {
      this.history = []
      localStorage.removeItem('chat_history')
    },
    
    /**
     * 从 localStorage 加载聊天记录
     */
    loadFromStorage() {
      try {
        const saved = localStorage.getItem('chat_history')
        if (saved) {
          this.history = JSON.parse(saved)
        }
      } catch (e) {
        console.warn('Failed to load chat history:', e)
      }
    },
    
    /**
     * 保存聊天记录到 localStorage
     */
    saveToStorage() {
      try {
        localStorage.setItem('chat_history', JSON.stringify(this.history.slice(-MAX_CHAT_HISTORY)))
      } catch (e) {
        console.warn('Failed to save chat history:', e)
      }
    },
    
    /**
     * 导出聊天记录
     * @returns {string} 导出的文本内容
     */
    exportHistory() {
      return this.history.map(msg =>
        `[${msg.role === 'user' ? '👤' : '🤖'} ${new Date(msg.timestamp).toLocaleString()}]\n${msg.message}`
      ).join('\n\n')
    }
  }
})
