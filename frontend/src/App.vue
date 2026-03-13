<template>
  <div class="app">
    <header class="header">
      <h1>🏗️ CAD 图纸识别</h1>
      <p class="subtitle">基于 AI 的智能图纸分析系统</p>
      <button class="settings-btn" @click="showSettings = !showSettings">
        {{ showSettings ? '隐藏设置' : '⚙️ 设置' }}
      </button>
    </header>

    <!-- 设置区域 -->
    <SettingsSection
      v-if="showSettings"
      :model-value="apiStore.apiKey"
      :saved="apiStore.apiKeySaved"
      :loading="uploadStore.loading"
      :quota-info="apiStore.quotaInfo"
      @save="saveApiKey"
      @generate="generateNewKey"
      @clear="clearApiKey"
    />

    <main class="main-content">
      <!-- 上传区域 -->
      <section class="upload-section">
        <UploadZone
          :max-file-size="MAX_FILE_SIZE"
          @update:files="handleFileUpdate"
          @error="toast.error($event)"
        />

        <!-- 自定义问题输入 -->
        <div class="question-section">
          <label for="custom-question">💬 分析提示词:</label>
          <input
            id="custom-question"
            v-model="customQuestion"
            type="text"
            placeholder="例如：提取图纸中的所有尺寸标注和材料信息..."
            class="question-input"
          />
        </div>

        <button
          type="button"
          class="analyze-btn"
          :disabled="!uploadStore.selectedFiles.length || uploadStore.loading"
          @click="analyze"
        >
          {{ uploadStore.loading ? '分析中...' : '开始分析' }}
        </button>
      </section>

      <!-- 加载状态 -->
      <LoadingSection
        v-if="uploadStore.loading"
        :text="uploadStore.loadingText"
        :progress="uploadStore.progress"
      />

      <!-- 结果展示 -->
      <ResultSection
        v-if="uploadStore.result"
        :result="uploadStore.result"
        @copy="toast.success('已复制到剪贴板')"
      />

      <!-- 对话区域 -->
      <ChatSection
        v-if="uploadStore.result || chatStore.history.length"
        :history="chatStore.history"
        :loading="uploadStore.loading"
        @send="sendChat"
        @clear="clearChatHistory"
        @export="exportChatHistory"
      />
    </main>

    <footer class="footer">
      <p>CAD OCR v0.10.0 | Powered by Vue 3 + Rust</p>
    </footer>
  </div>
</template>

<script setup>
import { ref, onMounted, watch } from 'vue'
import api from './api.js'
import ChatSection from './components/ChatSection.vue'
import LoadingSection from './components/LoadingSection.vue'
import ResultSection from './components/ResultSection.vue'
import SettingsSection from './components/SettingsSection.vue'
import UploadZone from './components/UploadZone.vue'
import { PROGRESS_BASE, PROGRESS_SCALE, MAX_FILE_SIZE } from './config/constants.js'
import { useApiStore, useUploadStore, useChatStore } from './stores'
import { handleApiError } from './utils/error.js'
import toast from './utils/toast.js'

const apiStore = useApiStore()
const uploadStore = useUploadStore()
const chatStore = useChatStore()

// 调试：监听 selectedFiles 变化
watch(() => uploadStore.selectedFiles, (newVal, oldVal) => {
  console.log('[watch] selectedFiles 变化:', { old: oldVal?.length, new: newVal?.length, newVal })
}, { deep: true })

const showSettings = ref(false)
const customQuestion = ref('')

// 处理文件更新（带调试）
function handleFileUpdate(files) {
  console.log('[App] 收到文件更新事件，files:', files)
  uploadStore.setFiles(files)
  console.log('[App] uploadStore.selectedFiles 更新后:', uploadStore.selectedFiles)
}

// 文件转 base64
function fileToBase64(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(reader.result.split(',')[1])
    reader.onerror = reject
    reader.readAsDataURL(file)
  })
}

// 带进度的上传
async function uploadWithProgress() {
  const formData = new FormData()
  formData.append('image', uploadStore.selectedFiles[0])
  formData.append('drawing_type', customQuestion.value ? '自定义' : '建筑平面图')
  formData.append('question', customQuestion.value || '请详细分析这张图纸，提取所有关键信息')

  return new Promise((resolve, reject) => {
    api.analyzeImage(formData, {
      onUploadProgress: (progressEvent) => {
        const percent = Math.round((progressEvent.loaded * 100) / progressEvent.total)
        uploadStore.progress = PROGRESS_BASE + (percent * PROGRESS_SCALE)
        uploadStore.loadingText = `上传中... ${percent}%`
      }
    })
    .then(response => {
      const result = response.data.response || response.data.message
      uploadStore.setResult(result)
      uploadStore.progress = 100

      chatStore.addMessage(
        `分析文件：${uploadStore.selectedFiles.map(f => f.name).join(', ')}`,
        'user'
      )
      chatStore.addMessage(result, 'assistant')

      resolve()
    })
    .catch(reject)
  })
}

// 加载配额
function loadQuota() {
  api.getQuota()
    .then(res => {
      apiStore.setQuotaInfo(res.data.data || res.data)
    })
    .catch(err => {
      if (apiStore.apiKeySaved) {
        toast.warning('配额信息加载失败，API Key 可能无效')
      }
      console.warn('Failed to load quota info:', err)
    })
}

// 保存 API Key
function saveApiKey() {
  try {
    apiStore.saveApiKey(apiStore.apiKey)
    loadQuota()
    toast.success('API Key 已保存')
  } catch (err) {
    toast.error(err.message)
  }
}

// 清除 API Key
function clearApiKey() {
  apiStore.clearApiKey()
  toast.info('API Key 已清除')
}

// 生成新 Key
async function generateNewKey() {
  uploadStore.setLoading(true, '正在生成 API Key...')
  try {
    const res = await api.createApiKey('Web User', 100)
    const newKey = res.data.data?.key || res.data.key
    apiStore.saveApiKey(newKey)
    loadQuota()

    await navigator.clipboard.writeText(newKey)
    toast.success('已自动复制到剪贴板')
  } catch (err) {
    handleApiError(err, { showToast: true })
  } finally {
    uploadStore.setLoading(false)
  }
}

// 开始分析
async function analyze() {
  console.log('[analyze] 被调用')
  console.log('[analyze] selectedFiles 长度:', uploadStore.selectedFiles?.length)
  console.log('[analyze] selectedFiles 内容:', uploadStore.selectedFiles)
  
  if (!uploadStore.selectedFiles.length) {
    console.log('[analyze] 没有文件，返回')
    toast.warning('请先选择文件')
    return
  }
  console.log('[analyze] 开始分析...')

  uploadStore.setLoading(true, '准备上传...')

  try {
    // 并发转换所有文件
    uploadStore.setLoading(true, '正在处理图片...')
    const images = await Promise.all(
      uploadStore.selectedFiles.map(file => fileToBase64(file))
    )
    uploadStore.setImages(images)

    // 真实上传进度
    uploadStore.setLoading(true, '正在上传分析...')
    await uploadWithProgress()

    toast.success('分析完成')
    chatStore.saveToStorage()
    loadQuota()

  } catch (err) {
    handleApiError(err, { showToast: true })
  } finally {
    setTimeout(() => {
      uploadStore.setLoading(false)
    }, 1000)
  }
}

// 发送消息
async function sendChat(message) {
  if (!message || !uploadStore.images.length) return

  uploadStore.setLoading(true)
  try {
    const response = await api.chat(message, uploadStore.images)
    const reply = response.data.response || response.data.message

    chatStore.addMessage(message, 'user')
    chatStore.addMessage(reply, 'assistant')
  } catch (err) {
    handleApiError(err, { showToast: true })
  } finally {
    uploadStore.setLoading(false)
  }
}

// 清空聊天历史
function clearChatHistory() {
  chatStore.clearHistory()
  toast.success('聊天记录已清空')
}

// 导出聊天历史
function exportChatHistory() {
  const content = chatStore.history
    .map(msg => `[${msg.role === 'user' ? '我' : 'AI'}] ${msg.message}`)
    .join('\n\n')
  const blob = new Blob([content], { type: 'text/plain' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `对话记录-${new Date().toLocaleString().replace(/[/:]/g, '-')}.txt`
  a.click()
  URL.revokeObjectURL(url)
  toast.success('已导出聊天记录')
}

// 挂载时加载数据
onMounted(() => {
  if (apiStore.loadApiKey()) {
    loadQuota()
  }
  chatStore.loadFromStorage()
})
</script>

<style scoped>
* { margin: 0; padding: 0; box-sizing: border-box; }
.app { min-height: 100vh; display: flex; flex-direction: column; background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; }
.header { text-align: center; padding: 2rem; color: white; position: relative; }
.header h1 { font-size: 2.5rem; margin-bottom: 0.5rem; }
.subtitle { opacity: 0.9; font-size: 1.1rem; }
.settings-btn { position: absolute; top: 1rem; right: 1rem; background: rgba(255,255,255,0.2); border: 1px solid rgba(255,255,255,0.3); color: white; padding: 0.5rem 1rem; border-radius: 8px; cursor: pointer; font-size: 0.9rem; transition: all 0.2s; }
.settings-btn:hover { background: rgba(255,255,255,0.3); }
.main-content { flex: 1; max-width: 900px; width: 100%; margin: 0 auto; padding: 1rem; }
.upload-section { background: white; border-radius: 16px; padding: 0; margin-bottom: 1.5rem; box-shadow: 0 10px 40px rgba(0,0,0,0.1); }
.question-section { padding: 1rem 1.5rem 0; }
.question-section label { display: block; margin-bottom: 0.5rem; color: #555; font-weight: 500; font-size: 0.95rem; }
.question-input { width: 100%; padding: 0.75rem 1rem; border: 2px solid #e0e0e0; border-radius: 8px; font-size: 0.95rem; transition: border-color 0.2s; }
.question-input:focus { outline: none; border-color: #667eea; }
.analyze-btn { width: 100%; margin: 1rem 0 1.5rem; padding: 1rem; background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); color: white; border: none; border-radius: 8px; font-size: 1.1rem; font-weight: 600; cursor: pointer; transition: transform 0.2s; }
.analyze-btn:hover:not(:disabled) { transform: translateY(-2px); }
.analyze-btn:disabled { opacity: 0.6; cursor: not-allowed; }
.footer { text-align: center; padding: 1.5rem; color: rgba(255,255,255,0.8); font-size: 0.9rem; }
@media (max-width: 600px) { .header h1 { font-size: 1.8rem; } .upload-section, .result-section, .chat-section { padding: 1.5rem; } }
</style>
