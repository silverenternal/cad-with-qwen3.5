import { createPinia, setActivePinia } from 'pinia'
import { describe, it, expect, beforeEach } from 'vitest'

import { useUploadStore, useChatStore } from './upload'

describe('stores/upload.js', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('应该初始化默认状态', () => {
    const store = useUploadStore()
    expect(store.selectedFiles).toEqual([])
    expect(store.loading).toBe(false)
    expect(store.loadingText).toBe('')
    expect(store.progress).toBe(0)
    expect(store.result).toBeNull()
    expect(store.images).toEqual([])
  })

  describe('hasFiles getter', () => {
    it('当有文件时返回 true', () => {
      const store = useUploadStore()
      store.selectedFiles = [{ name: 'test.pdf' }]
      expect(store.hasFiles).toBe(true)
    })

    it('当没有文件时返回 false', () => {
      const store = useUploadStore()
      expect(store.hasFiles).toBe(false)
    })
  })

  describe('isLoading getter', () => {
    it('应该返回加载状态', () => {
      const store = useUploadStore()
      expect(store.isLoading).toBe(false)

      store.loading = true
      expect(store.isLoading).toBe(true)
    })
  })

  describe('setFiles action', () => {
    it('应该设置选中的文件', () => {
      const store = useUploadStore()
      const files = [{ name: 'test1.pdf' }, { name: 'test2.jpg' }]

      store.setFiles(files)

      expect(store.selectedFiles).toEqual(files)
    })
  })

  describe('clearFiles action', () => {
    it('应该清除文件', () => {
      const store = useUploadStore()
      store.selectedFiles = [{ name: 'test.pdf' }]

      store.clearFiles()

      expect(store.selectedFiles).toEqual([])
    })
  })

  describe('setLoading action', () => {
    it('应该设置加载状态', () => {
      const store = useUploadStore()

      store.setLoading(true, '上传中...', 50)

      expect(store.loading).toBe(true)
      expect(store.loadingText).toBe('上传中...')
      expect(store.progress).toBe(50)
    })
  })

  describe('setResult action', () => {
    it('应该设置分析结果', () => {
      const store = useUploadStore()
      const result = '这是分析结果'

      store.setResult(result)

      expect(store.result).toBe(result)
    })
  })

  describe('clearResult action', () => {
    it('应该清除分析结果', () => {
      const store = useUploadStore()
      store.result = '测试结果'

      store.clearResult()

      expect(store.result).toBeNull()
    })
  })

  describe('setImages action', () => {
    it('应该设置 Base64 图片', () => {
      const store = useUploadStore()
      const images = ['data:image/png;base64,...']

      store.setImages(images)

      expect(store.images).toEqual(images)
    })
  })

  describe('reset action', () => {
    it('应该重置上传状态', () => {
      const store = useUploadStore()
      store.loading = true
      store.loadingText = '测试'
      store.progress = 50
      store.result = '结果'
      store.images = ['test']

      store.reset()

      expect(store.loading).toBe(false)
      expect(store.loadingText).toBe('')
      expect(store.progress).toBe(0)
      expect(store.result).toBeNull()
      expect(store.images).toEqual([])
    })
  })
})

describe('stores/chat.js', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
  })

  it('应该初始化默认状态', () => {
    const store = useChatStore()
    expect(store.history).toEqual([])
    expect(store.message).toBe('')
  })

  describe('hasHistory getter', () => {
    it('当有历史记录时返回 true', () => {
      const store = useChatStore()
      store.history = [{ message: 'test', role: 'user' }]
      expect(store.hasHistory).toBe(true)
    })

    it('当没有历史记录时返回 false', () => {
      const store = useChatStore()
      expect(store.hasHistory).toBe(false)
    })
  })

  describe('addMessage action', () => {
    it('应该添加消息到历史记录', () => {
      const store = useChatStore()

      store.addMessage('你好', 'user')

      expect(store.history.length).toBe(1)
      expect(store.history[0]).toMatchObject({
        message: '你好',
        role: 'user'
      })
      expect(store.history[0].timestamp).toBeDefined()
    })

    it('应该限制历史记录数量', () => {
      const store = useChatStore()

      // 添加超过 MAX_CHAT_HISTORY 的消息
      for (let i = 0; i < 60; i++) {
        store.addMessage(`消息${i}`, 'user')
      }

      expect(store.history.length).toBeLessThanOrEqual(50)
    })
  })

  describe('clearHistory action', () => {
    it('应该清空聊天记录', () => {
      const store = useChatStore()
      store.history = [{ message: 'test', role: 'user' }]

      store.clearHistory()

      expect(store.history).toEqual([])
      expect(localStorage.getItem('chat_history')).toBeNull()
    })
  })

  describe('exportHistory action', () => {
    it('应该导出聊天记录', () => {
      const store = useChatStore()
      store.history = [
        { message: '你好', role: 'user', timestamp: Date.now() },
        { message: '你好！有什么可以帮助你的吗？', role: 'assistant', timestamp: Date.now() }
      ]

      const content = store.exportHistory()

      expect(content).toContain('👤')
      expect(content).toContain('🤖')
      expect(content).toContain('你好')
    })
  })
})
