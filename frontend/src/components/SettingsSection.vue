<template>
  <section class="settings-section">
    <!-- API Key 配置 -->
    <div class="settings-card">
      <h2>🔑 API Key 配置</h2>
      <div class="form-group">
        <label for="api-key">API Key:</label>
        <input
          id="api-key"
          type="password"
          :value="modelValue"
          placeholder="请输入 API Key（如：sk_xxx...）"
          @input="$emit('update:modelValue', $event.target.value)"
        />
      </div>
      <div class="form-actions">
        <button type="button" class="btn-primary" @click="$emit('save')">保存</button>
        <button type="button" :disabled="loading" class="btn-secondary" @click="$emit('generate')">
          {{ loading ? '生成中...' : '生成新 Key' }}
        </button>
        <button type="button" class="btn-danger" @click="$emit('clear')">清除</button>
      </div>
      <div class="api-key-status" :class="saved ? 'saved' : 'missing'">
        <span class="status-dot" :class="saved ? 'saved' : 'missing'"></span>
        <span>{{ statusText }}</span>
      </div>
    </div>

    <!-- 配额信息 -->
    <div v-if="quotaInfo" class="settings-card">
      <h2>📊 配额信息</h2>
      <div class="quota-grid">
        <div class="quota-item">
          <span class="quota-label">每日限额:</span>
          <span class="quota-value">{{ quotaInfo.daily_limit }} 次</span>
        </div>
        <div class="quota-item">
          <span class="quota-label">已用:</span>
          <span class="quota-value">{{ quotaInfo.used_today }} 次</span>
        </div>
        <div class="quota-item">
          <span class="quota-label">剩余:</span>
          <span class="quota-value" :class="{ 'quota-warning': quotaInfo.remaining === 0 }">
            {{ quotaInfo.remaining }} 次
          </span>
        </div>
      </div>
    </div>
  </section>
</template>

<script>
/**
 * 设置面板组件
 */
export default {
  name: 'SettingsSection',
  props: {
    modelValue: {
      type: String,
      default: ''
    },
    saved: {
      type: Boolean,
      default: false
    },
    loading: {
      type: Boolean,
      default: false
    },
    quotaInfo: {
      type: Object,
      default: null
    }
  },
  emits: ['update:modelValue', 'save', 'generate', 'clear'],
  computed: {
    statusText() {
      if (this.saved) {
        const key = this.modelValue
        if (!key) return '已保存 API Key'
        return `已保存 API Key: ${key.substring(0, 8)}...`
      }
      return '未配置 API Key，部分功能可能不可用'
    }
  }
}
</script>

<style scoped>
.settings-section {
  max-width: 900px;
  margin: 0 auto 1.5rem;
  padding: 0 1rem;
}

.settings-card {
  background: white;
  border-radius: 16px;
  padding: 1.5rem;
  margin-bottom: 1rem;
  box-shadow: 0 10px 40px rgba(0, 0, 0, 0.1);
}

.settings-card h2 {
  color: #333;
  font-size: 1.2rem;
  margin: 0 0 1rem;
}

.form-group {
  margin-bottom: 1rem;
}

.form-group label {
  display: block;
  margin-bottom: 0.5rem;
  color: #555;
  font-weight: 500;
}

.form-group input {
  width: 100%;
  padding: 0.75rem 1rem;
  border: 2px solid #e0e0e0;
  border-radius: 8px;
  font-size: 1rem;
  font-family: monospace;
  transition: border-color 0.2s;
}

.form-group input:focus {
  outline: none;
  border-color: #667eea;
}

.form-actions {
  display: flex;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.btn-primary {
  padding: 0.6rem 1.2rem;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  border: none;
  border-radius: 8px;
  font-weight: 600;
  cursor: pointer;
  transition: transform 0.2s;
}

.btn-primary:hover:not(:disabled) {
  transform: translateY(-2px);
}

.btn-secondary {
  padding: 0.6rem 1.2rem;
  background: #f0f2ff;
  color: #667eea;
  border: 2px solid #667eea;
  border-radius: 8px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s;
}

.btn-secondary:hover:not(:disabled) {
  background: #667eea;
  color: white;
}

.btn-danger {
  padding: 0.6rem 1.2rem;
  background: #fff5f5;
  color: #ff4757;
  border: 2px solid #ff4757;
  border-radius: 8px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s;
}

.btn-danger:hover:not(:disabled) {
  background: #ff4757;
  color: white;
}

.api-key-status {
  margin-top: 1rem;
  padding: 0.75rem 1rem;
  background: #f8f9ff;
  border-radius: 8px;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.9rem;
}

.api-key-status.saved {
  background: #f0fdf4;
}

.api-key-status.missing {
  background: #fffbeb;
}

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}

.status-dot.saved {
  background: #10b981;
}

.status-dot.missing {
  background: #f59e0b;
}

.quota-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
  gap: 1rem;
}

.quota-item {
  display: flex;
  flex-direction: column;
  padding: 0.75rem;
  background: #f8f9ff;
  border-radius: 8px;
}

.quota-label {
  font-size: 0.8rem;
  color: #888;
  margin-bottom: 0.25rem;
}

.quota-value {
  font-size: 1.2rem;
  font-weight: 600;
  color: #333;
}

.quota-warning {
  color: #ff4757;
}

@media (max-width: 600px) {
  .form-actions {
    flex-direction: column;
  }

  .btn-primary,
  .btn-secondary,
  .btn-danger {
    width: 100%;
  }
}
</style>
