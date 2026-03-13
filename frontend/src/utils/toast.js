/**
 * Toast 通知工具 - 修复内存泄漏版本
 * 
 * 使用方法:
 * import { toast } from '@/utils/toast'
 * toast.success('操作成功')
 */

import { createApp, h, Teleport } from 'vue'

import Toast from '../components/Toast.vue'

/**
 * Toast 容器组件 - 管理所有 Toast 实例
 */
const ToastContainer = {
  name: 'ToastContainer',
  data() {
    return {
      toasts: []
    }
  },
  methods: {
    add(message, type = 'info', duration = 3000) {
      const id = Date.now() + Math.random()
      const toast = { id, message, type, duration }
      this.toasts.push(toast)
      
      // 自动移除
      if (duration > 0) {
        setTimeout(() => this.remove(id), duration)
      }
      
      return id
    },
    remove(id) {
      const index = this.toasts.findIndex(t => t.id === id)
      if (index > -1) {
        this.toasts.splice(index, 1)
      }
    },
    clear() {
      this.toasts = []
    }
  },
  render() {
    return h(
      Teleport,
      { to: 'body' },
      h('div', { class: 'toast-container-fixed' },
        this.toasts.map(toast =>
          h(Toast, {
            key: toast.id,
            message: toast.message,
            type: toast.type,
            duration: toast.duration,
            onClose: () => this.remove(toast.id)
          })
        )
      )
    )
  }
}

// 创建全局容器
let containerInstance = null

function getContainer() {
  if (!containerInstance) {
    const container = document.createElement('div')
    container.id = 'toast-root'
    document.body.appendChild(container)
    
    const app = createApp(ToastContainer)
    containerInstance = app.mount(container)
  }
  return containerInstance
}

export const toast = {
  success(message, duration = 3000) {
    return getContainer().add(message, 'success', duration)
  },
  error(message, duration = 3000) {
    return getContainer().add(message, 'error', duration)
  },
  warning(message, duration = 3000) {
    return getContainer().add(message, 'warning', duration)
  },
  info(message, duration = 3000) {
    return getContainer().add(message, 'info', duration)
  },
  clearAll() {
    getContainer()?.clear()
  }
}

export default toast
