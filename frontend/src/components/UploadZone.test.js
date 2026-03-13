import { mount } from '@vue/test-utils'
import { describe, it, expect, beforeEach } from 'vitest'

import UploadZone from '../components/UploadZone.vue'

import { MAX_FILE_SIZE } from '@/config/constants'


describe('UploadZone.vue', () => {
  let wrapper

  beforeEach(() => {
    wrapper = mount(UploadZone, {
      props: {
        maxFileSize: MAX_FILE_SIZE
      }
    })
  })

  it('应该渲染上传区域', () => {
    expect(wrapper.find('.upload-zone').exists()).toBe(true)
  })

  it('应该显示默认提示文字', () => {
    expect(wrapper.text()).toContain('拖拽文件到此处，或点击选择文件')
  })

  it('应该支持文件类型提示', () => {
    expect(wrapper.text()).toContain('支持 JPG, PNG, GIF, WebP, BMP, PDF')
  })

  it('应该可以触发文件选择', async () => {
    const input = wrapper.find('.file-input')
    expect(input.exists()).toBe(true)
  })

  it('应该显示添加和清空按钮（当有文件时）', async () => {
    // 初始没有按钮
    expect(wrapper.find('.upload-actions').exists()).toBe(false)

    // 模拟添加文件
    await wrapper.vm.addFiles([
      new File(['test'], 'test.jpg', { type: 'image/jpeg' })
    ])
    await wrapper.vm.$nextTick()

    expect(wrapper.find('.upload-actions').exists()).toBe(true)
  })

  it('应该能添加文件', async () => {
    const file = new File(['test content'], 'test.jpg', { type: 'image/jpeg' })
    await wrapper.vm.addFiles([file])

    expect(wrapper.vm.files.length).toBe(1)
    expect(wrapper.emitted('update:files')).toBeDefined()
  })

  it('应该拒绝不支持的文件类型', async () => {
    const file = new File(['test'], 'test.txt', { type: 'text/plain' })
    await wrapper.vm.addFiles([file])

    expect(wrapper.emitted('error')).toBeDefined()
    expect(wrapper.emitted('error')[0][0]).toContain('不支持的文件类型')
  })

  it('应该拒绝过大的文件', async () => {
    const file = new File(['x'.repeat(MAX_FILE_SIZE + 1024 * 1024)], 'large.jpg', { type: 'image/jpeg' })
    await wrapper.vm.addFiles([file])

    expect(wrapper.emitted('error')).toBeDefined()
    expect(wrapper.emitted('error')[0][0]).toContain('文件过大')
  })

  it('应该能移除文件', async () => {
    const file = new File(['test'], 'test.jpg', { type: 'image/jpeg' })
    await wrapper.vm.addFiles([file])
    expect(wrapper.vm.files.length).toBe(1)

    await wrapper.vm.removeFile(0)
    expect(wrapper.vm.files.length).toBe(0)
  })

  it('应该能清空所有文件', async () => {
    await wrapper.vm.addFiles([
      new File(['test1'], 'test1.jpg', { type: 'image/jpeg' }),
      new File(['test2'], 'test2.png', { type: 'image/png' })
    ])
    expect(wrapper.vm.files.length).toBe(2)

    await wrapper.vm.clearAll()
    expect(wrapper.vm.files.length).toBe(0)
  })

  it('应该能格式化文件大小', () => {
    expect(wrapper.vm.formatFileSize(0)).toBe('0 B')
    expect(wrapper.vm.formatFileSize(1024)).toBe('1 KB')
    expect(wrapper.vm.formatFileSize(1024 * 1024)).toBe('1 MB')
  })

  it('应该处理拖拽事件', async () => {
    expect(wrapper.vm.isDragOver).toBe(false)

    await wrapper.trigger('dragover', { preventDefault: () => {} })
    expect(wrapper.vm.isDragOver).toBe(true)

    await wrapper.trigger('dragleave', { preventDefault: () => {} })
    expect(wrapper.vm.isDragOver).toBe(false)
  })

  it('应该处理拖拽放置', async () => {
    const file = new File(['test'], 'test.jpg', { type: 'image/jpeg' })
    const dataTransfer = {
      files: [file]
    }

    await wrapper.trigger('drop', {
      preventDefault: () => {},
      dataTransfer
    })

    expect(wrapper.vm.files.length).toBeGreaterThan(0)
  })

  it('应该在卸载前清理所有 URL', async () => {
    const revokeObjectURL = vi.spyOn(URL, 'revokeObjectURL')

    await wrapper.vm.addFiles([
      new File(['test'], 'test.jpg', { type: 'image/jpeg' })
    ])

    await wrapper.unmount()

    expect(revokeObjectURL).toHaveBeenCalled()
    revokeObjectURL.mockRestore()
  })
})
