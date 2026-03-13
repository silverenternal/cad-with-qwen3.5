<template>
  <section class="result-section">
    <div class="result-header">
      <h2>📊 分析结果</h2>
      <button type="button" class="copy-btn" :title="copied ? '已复制' : '复制结果'" @click="handleCopy">
        {{ copied ? '✅ 已复制' : '📋 复制' }}
      </button>
    </div>
    <div class="result-content">
      <div class="ai-response" v-html="formattedContent"></div>
    </div>
  </section>
</template>

<script>
import DOMPurify from 'dompurify'

/**
 * 结果展示组件
 */
export default {
  name: 'ResultSection',
  props: {
    result: {
      type: String,
      required: true
    }
  },
  emits: ['copy'],
  data() {
    return {
      copied: false,
      copyTimer: null
    }
  },
  computed: {
    formattedContent() {
      if (!this.result) return ''
      // 使用 DOMPurify  sanitization 防止 XSS 攻击
      const sanitized = DOMPurify.sanitize(this.result)
      return sanitized
        .replace(/\n/g, '<br>')
        .replace(/^- /gm, '<li>')
    }
  },
  beforeUnmount() {
    if (this.copyTimer) clearTimeout(this.copyTimer)
  },
  methods: {
    async handleCopy() {
      try {
        await navigator.clipboard.writeText(this.result)
        this.copied = true
        this.$emit('copy')
        
        if (this.copyTimer) clearTimeout(this.copyTimer)
        this.copyTimer = setTimeout(() => {
          this.copied = false
        }, 2000)
      } catch (err) {
        console.error('复制失败:', err)
      }
    }
  }
}
</script>

<style scoped>
.result-section {
  background: white;
  border-radius: 16px;
  padding: 2rem;
  margin-bottom: 1.5rem;
  box-shadow: 0 10px 40px rgba(0, 0, 0, 0.1);
}

.result-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1rem;
}

.result-header h2 {
  color: #333;
  margin: 0;
}

.copy-btn {
  padding: 0.5rem 1rem;
  background: #667eea;
  color: white;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-weight: 500;
  transition: all 0.2s;
}

.copy-btn:hover {
  background: #5568d3;
  transform: translateY(-1px);
}

.result-content {
  background: #f8f9ff;
  padding: 1.5rem;
  border-radius: 8px;
  border-left: 4px solid #667eea;
}

.ai-response {
  line-height: 1.8;
  color: #333;
  font-size: 0.95rem;
}

.ai-response :deep(li) {
  margin-left: 1.5rem;
  margin-bottom: 0.5rem;
}
</style>
