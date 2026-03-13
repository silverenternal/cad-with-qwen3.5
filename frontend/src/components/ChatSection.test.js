import { mount } from '@vue/test-utils'
import { describe, it, expect } from 'vitest'

import ChatSection from '../components/ChatSection.vue'

describe('ChatSection.vue', () => {
  const mockHistory = [
    { role: 'user', message: '你好', timestamp: Date.now() },
    { role: 'assistant', message: '你好！有什么可以帮助你的吗？', timestamp: Date.now() }
  ]

  it('应该渲染聊天区域', () => {
    const wrapper = mount(ChatSection, {
      props: { history: mockHistory }
    })

    expect(wrapper.find('.chat-section').exists()).toBe(true)
  })

  it('应该显示聊天历史', () => {
    const wrapper = mount(ChatSection, {
      props: { history: mockHistory }
    })

    expect(wrapper.text()).toContain('你好')
    expect(wrapper.text()).toContain('有什么可以帮助你的吗')
  })

  it('应该区分用户和助手消息', () => {
    const wrapper = mount(ChatSection, {
      props: { history: mockHistory }
    })

    const messages = wrapper.findAll('.message')
    expect(messages.length).toBe(2)
  })

  it('应该显示空状态提示', () => {
    const wrapper = mount(ChatSection, {
      props: { history: [] }
    })

    expect(wrapper.text()).toContain('暂无对话')
  })

  it('应该能输入消息', () => {
    const wrapper = mount(ChatSection, {
      props: { history: [] }
    })

    const input = wrapper.find('input')
    expect(input.exists()).toBe(true)
  })

  it('应该能发送消息', async () => {
    const wrapper = mount(ChatSection, {
      props: { history: [] }
    })

    const input = wrapper.find('input')
    await input.setValue('测试消息')
    // 使用 handleSend 方法直接测试
    wrapper.vm.inputMessage = '测试消息'
    wrapper.vm.handleSend()

    expect(wrapper.emitted('send')).toBeDefined()
    expect(wrapper.emitted('send')[0][0]).toBe('测试消息')
  })

  it('应该能按 Enter 发送消息', async () => {
    const wrapper = mount(ChatSection, {
      props: { history: [] }
    })

    const input = wrapper.find('input')
    await input.setValue('测试消息')
    await input.trigger('keyup.enter')

    expect(wrapper.emitted('send')).toBeDefined()
  })

  it('应该在发送后清空输入框', async () => {
    const wrapper = mount(ChatSection, {
      props: { history: [] }
    })

    wrapper.vm.inputMessage = '测试消息'
    wrapper.vm.handleSend()

    expect(wrapper.vm.inputMessage).toBe('')
  })

  it('应该不能发送空消息', async () => {
    const wrapper = mount(ChatSection, {
      props: { history: [] }
    })

    wrapper.vm.inputMessage = ''
    wrapper.vm.handleSend()

    expect(wrapper.emitted('send')).not.toBeDefined()
  })

  it('应该能清空聊天记录', async () => {
    const wrapper = mount(ChatSection, {
      props: { history: mockHistory }
    })

    await wrapper.find('.btn-clear-chat').trigger('click')

    expect(wrapper.emitted('clear')).toBeDefined()
  })

  it('应该能导出聊天记录', async () => {
    const wrapper = mount(ChatSection, {
      props: { history: mockHistory }
    })

    await wrapper.find('.btn-export').trigger('click')

    expect(wrapper.emitted('export')).toBeDefined()
  })

  it('应该显示加载状态', () => {
    const wrapper = mount(ChatSection, {
      props: { history: [], loading: true }
    })

    const input = wrapper.find('input')
    expect(input.attributes('disabled')).toBeDefined()
  })

  it('应该在加载时禁用发送按钮', async () => {
    const wrapper = mount(ChatSection, {
      props: { history: [], loading: true }
    })

    await wrapper.vm.$nextTick()
    
    // 聊天输入区域的按钮文本应该显示"发送中..."
    const sendButton = wrapper.find('.chat-input-area button')
    expect(sendButton.text()).toBe('发送中...')
  })
})
