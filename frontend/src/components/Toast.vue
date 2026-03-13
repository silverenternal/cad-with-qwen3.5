<template>
  <div class="toast" :class="type" role="alert">
    <span class="toast-icon">{{ icon }}</span>
    <span class="toast-message">{{ message }}</span>
    <button type="button" class="toast-close" aria-label="关闭" @click="handleClose">×</button>
  </div>
</template>

<script>
export default {
  name: 'Toast',
  props: {
    type: {
      type: String,
      default: 'info',
      validator: (value) => ['success', 'error', 'warning', 'info'].includes(value)
    },
    message: {
      type: String,
      required: true
    },
    duration: {
      type: Number,
      default: 3000
    }
  },
  emits: ['close'],
  data() {
    return {
      timer: null
    }
  },
  computed: {
    icon() {
      const icons = {
        success: '✅',
        error: '❌',
        warning: '⚠️',
        info: 'ℹ️'
      }
      return icons[this.type] || icons.info
    }
  },
  mounted() {
    if (this.duration > 0) {
      this.timer = setTimeout(this.handleClose, this.duration)
    }
  },
  beforeUnmount() {
    if (this.timer) clearTimeout(this.timer)
  },
  methods: {
    handleClose() {
      if (this.timer) clearTimeout(this.timer)
      this.$emit('close')
    }
  }
}
</script>

<style scoped>
.toast {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 1rem 1.25rem;
  background: white;
  border-radius: 12px;
  box-shadow: 0 10px 40px rgba(0, 0, 0, 0.15);
  min-width: 300px;
  max-width: 500px;
  border-left: 4px solid #667eea;
  animation: slideIn 0.3s ease;
}

@keyframes slideIn {
  from {
    transform: translateX(100%);
    opacity: 0;
  }
  to {
    transform: translateX(0);
    opacity: 1;
  }
}

.toast.success {
  border-left-color: #10b981;
}

.toast.error {
  border-left-color: #ff4757;
}

.toast.warning {
  border-left-color: #f59e0b;
}

.toast.info {
  border-left-color: #667eea;
}

.toast-icon {
  font-size: 1.25rem;
  flex-shrink: 0;
}

.toast-message {
  flex: 1;
  color: #333;
  font-size: 0.95rem;
  line-height: 1.5;
}

.toast-close {
  background: none;
  border: none;
  color: #999;
  font-size: 1.25rem;
  cursor: pointer;
  padding: 0;
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  transition: all 0.2s;
}

.toast-close:hover {
  background: #f0f0f0;
  color: #333;
}

/* 全局容器样式 */
.toast-container-fixed {
  position: fixed;
  top: 1rem;
  right: 1rem;
  z-index: 9999;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

@media (max-width: 600px) {
  .toast-container-fixed {
    top: 0.5rem;
    right: 0.5rem;
    left: 0.5rem;
  }

  .toast {
    min-width: auto;
    width: 100%;
  }
}
</style>
