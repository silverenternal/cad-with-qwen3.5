<template>
  <section class="chat-section">
    <div class="chat-header">
      <h2>💬 对话历史</h2>
      <div class="chat-actions">
        <button type="button" class="btn-export" title="导出聊天记录" @click="$emit('export')">
          📥 导出
        </button>
        <button type="button" class="btn-clear-chat" title="清空聊天记录" @click="$emit('clear')">
          🗑️ 清空
        </button>
      </div>
    </div>
    
    <div ref="chatContainer" class="chat-messages">
      <div 
        v-for="(msg, i) in history" 
        :key="i" 
        class="message" 
        :class="msg.role"
      >
        <div class="message-content">{{ msg.message }}</div>
        <div v-if="msg.timestamp" class="message-time">
          {{ formatTime(msg.timestamp) }}
        </div>
      </div>
      <div v-if="!history.length" class="empty-chat">
        💭 暂无对话，开始提问吧...
      </div>
    </div>
    
    <div class="chat-input-area">
      <input
        v-model="inputMessage"
        type="text"
        placeholder="输入问题，例如：这个结构的尺寸是多少？"
        :disabled="loading"
        @keyup.enter="handleSend"
      />
      <button type="button" :disabled="!inputMessage.trim() || loading" @click="handleSend">
        {{ loading ? '发送中...' : '发送' }}
      </button>
    </div>
  </section>
</template>

<script>
/**
 * 对话历史组件
 */
export default {
  name: 'ChatSection',
  props: {
    history: {
      type: Array,
      default: () => []
    },
    loading: {
      type: Boolean,
      default: false
    }
  },
  emits: ['send', 'clear', 'export'],
  data() {
    return {
      inputMessage: ''
    }
  },
  watch: {
    history: {
      handler() {
        this.$nextTick(() => {
          this.scrollToBottom()
        })
      },
      deep: true
    }
  },
  methods: {
    formatTime(timestamp) {
      const date = new Date(timestamp)
      const now = new Date()
      const diff = now - date
      
      if (diff < 60000) return '刚刚'
      if (diff < 3600000) return `${Math.floor(diff / 60000)}分钟前`
      if (date.toDateString() === now.toDateString()) {
        return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
      }
      return date.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
    },
    handleSend() {
      if (!this.inputMessage.trim()) return
      this.$emit('send', this.inputMessage.trim())
      this.inputMessage = ''
    },
    scrollToBottom() {
      if (this.$refs.chatContainer) {
        this.$refs.chatContainer.scrollTop = this.$refs.chatContainer.scrollHeight
      }
    }
  }
}
</script>

<style scoped>
.chat-section {
  background: white;
  border-radius: 16px;
  padding: 2rem;
  margin-bottom: 1.5rem;
  box-shadow: 0 10px 40px rgba(0, 0, 0, 0.1);
}

.chat-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1rem;
}

.chat-header h2 {
  color: #333;
  margin: 0;
}

.chat-actions {
  display: flex;
  gap: 0.5rem;
}

.btn-export,
.btn-clear-chat {
  padding: 0.4rem 0.8rem;
  border: 2px solid;
  border-radius: 6px;
  font-size: 0.85rem;
  cursor: pointer;
  transition: all 0.2s;
}

.btn-export {
  background: #f0f2ff;
  color: #667eea;
  border-color: #667eea;
}

.btn-export:hover {
  background: #667eea;
  color: white;
}

.btn-clear-chat {
  background: #fff5f5;
  color: #ff4757;
  border-color: #ff4757;
}

.btn-clear-chat:hover {
  background: #ff4757;
  color: white;
}

.chat-messages {
  max-height: 300px;
  overflow-y: auto;
  margin-bottom: 1rem;
  padding: 1rem;
  background: #f8f9ff;
  border-radius: 8px;
}

.empty-chat {
  text-align: center;
  color: #999;
  padding: 2rem;
}

.message {
  margin-bottom: 1rem;
  padding: 0.75rem 1rem;
  border-radius: 8px;
  max-width: 80%;
  position: relative;
  animation: fadeIn 0.3s ease;
}

@keyframes fadeIn {
  from {
    opacity: 0;
    transform: translateY(10px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.message.user {
  background: #667eea;
  color: white;
  margin-left: auto;
}

.message.assistant {
  background: #f0f2ff;
  color: #333;
}

.message-time {
  font-size: 0.7rem;
  opacity: 0.7;
  margin-top: 0.25rem;
  text-align: right;
}

.chat-input-area {
  display: flex;
  gap: 0.5rem;
}

.chat-input-area input {
  flex: 1;
  padding: 0.75rem 1rem;
  border: 2px solid #e0e0e0;
  border-radius: 8px;
  font-size: 1rem;
  transition: border-color 0.2s;
}

.chat-input-area input:focus {
  outline: none;
  border-color: #667eea;
}

.chat-input-area input:disabled {
  background: #f5f5f5;
  cursor: not-allowed;
}

.chat-input-area button {
  padding: 0.75rem 1.5rem;
  background: #667eea;
  color: white;
  border: none;
  border-radius: 8px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s;
}

.chat-input-area button:hover:not(:disabled) {
  background: #5568d3;
  transform: translateY(-1px);
}

.chat-input-area button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

@media (max-width: 600px) {
  .chat-header {
    flex-direction: column;
    align-items: flex-start;
    gap: 0.75rem;
  }

  .chat-actions {
    width: 100%;
  }

  .btn-export,
  .btn-clear-chat {
    flex: 1;
  }

  .chat-input-area {
    flex-direction: column;
  }

  .chat-input-area button {
    width: 100%;
  }
}
</style>
