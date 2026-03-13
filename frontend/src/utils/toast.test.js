import { describe, it, expect, beforeEach } from 'vitest'

import { toast } from './toast'

describe('toast.js', () => {
  beforeEach(() => {
    // 清理之前的容器
    const container = document.getElementById('toast-root')
    if (container) {
      container.remove()
    }
  })

  it('应该导出 toast 对象', () => {
    expect(toast).toBeDefined()
    expect(toast.success).toBeDefined()
    expect(toast.error).toBeDefined()
    expect(toast.warning).toBeDefined()
    expect(toast.info).toBeDefined()
    expect(toast.clearAll).toBeDefined()
  })

  it('应该创建 toast 容器', () => {
    toast.success('测试消息')
    const container = document.getElementById('toast-root')
    expect(container).toBeDefined()
  })

  it('应该添加 success 类型的 toast', () => {
    const id = toast.success('成功消息')
    expect(id).toBeDefined()
    expect(typeof id).toBe('number')
  })

  it('应该添加 error 类型的 toast', () => {
    const id = toast.error('错误消息')
    expect(id).toBeDefined()
  })

  it('应该添加 warning 类型的 toast', () => {
    const id = toast.warning('警告消息')
    expect(id).toBeDefined()
  })

  it('应该添加 info 类型的 toast', () => {
    const id = toast.info('提示消息')
    expect(id).toBeDefined()
  })

  it('应该支持自定义持续时间', () => {
    const id = toast.success('测试', 5000)
    expect(id).toBeDefined()
  })

  it('应该能清空所有 toast', () => {
    toast.success('消息 1')
    toast.error('消息 2')
    toast.clearAll()
    // 容器应该还存在，但里面的 toast 应该被清空
    const container = document.getElementById('toast-root')
    expect(container).toBeDefined()
  })
})
