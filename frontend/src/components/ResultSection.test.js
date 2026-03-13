import { mount } from '@vue/test-utils'
import { describe, it, expect } from 'vitest'

import ResultSection from '../components/ResultSection.vue'

describe('ResultSection.vue', () => {
  it('应该渲染分析结果', () => {
    const result = '这是分析结果'
    const wrapper = mount(ResultSection, {
      props: { result }
    })

    expect(wrapper.find('.result-section').exists()).toBe(true)
    expect(wrapper.text()).toContain('分析结果')
  })

  it('应该显示复制按钮', () => {
    const wrapper = mount(ResultSection, {
      props: { result: '测试结果' }
    })

    expect(wrapper.find('.copy-btn').exists()).toBe(true)
  })

  it('应该格式化内容（换行符转<br>）', () => {
    const result = '第一行\n第二行'
    const wrapper = mount(ResultSection, {
      props: { result }
    })

    const formattedContent = wrapper.vm.formattedContent
    expect(formattedContent).toContain('<br>')
  })

  it('应该使用 DOMPurify 净化内容', () => {
    const maliciousResult = '<script>alert("XSS")</script>正常内容'
    const wrapper = mount(ResultSection, {
      props: { result: maliciousResult }
    })

    const formattedContent = wrapper.vm.formattedContent
    // DOMPurify 应该移除 script 标签
    expect(formattedContent).not.toContain('<script>')
    expect(formattedContent).toContain('正常内容')
  })

  it('应该能复制结果', async () => {
    const result = '要复制的内容'
    const wrapper = mount(ResultSection, {
      props: { result }
    })

    // Mock clipboard API
    const mockWriteText = vi.fn().mockResolvedValue()
    Object.assign(navigator, { clipboard: { writeText: mockWriteText } })

    await wrapper.find('.copy-btn').trigger('click')

    expect(mockWriteText).toHaveBeenCalledWith(result)
    expect(wrapper.emitted('copy')).toBeDefined()
  })

  it('应该清空空结果', () => {
    const wrapper = mount(ResultSection, {
      props: { result: '' }
    })

    expect(wrapper.vm.formattedContent).toBe('')
  })
})
