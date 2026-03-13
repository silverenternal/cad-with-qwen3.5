<template>
  <div class="upload-zone" @dragover.prevent="isDragOver = true" @dragleave.prevent="isDragOver = false" @drop.prevent="handleDrop">
    <input
      v-show="!files.length"
      ref="fileInput"
      type="file"
      accept="image/*,.pdf"
      multiple
      class="file-input"
      @change.prevent="handleFileSelect"
    />
    
    <div v-if="!files.length" class="upload-placeholder" :class="{ 'drag-over': isDragOver }">
      <div class="upload-icon">📁</div>
      <p class="upload-text">拖拽文件到此处，或点击选择文件</p>
      <p class="upload-hint">支持 JPG, PNG, GIF, WebP, BMP, PDF</p>
    </div>

    <div v-else class="file-list">
      <div v-for="(file, index) in files" :key="index" class="file-item">
        <div v-if="file.preview" class="file-preview">
          <img :src="file.preview" :alt="file.name" />
        </div>
        <div v-else class="file-info">
          <span class="file-type-icon">📄</span>
        </div>
        <div class="file-details">
          <span class="file-name" :title="file.name">{{ file.name }}</span>
          <span class="file-size">{{ formatFileSize(file.size) }}</span>
        </div>
        <button type="button" class="remove-btn" title="删除" @click="removeFile(index)">
          ×
        </button>
      </div>
    </div>

    <div v-if="files.length" class="upload-actions">
      <button type="button" class="btn-add" @click="triggerFileInput">
        <span>➕</span> 添加更多
      </button>
      <button type="button" class="btn-clear" @click="clearAll">
        🗑️ 清空全部
      </button>
    </div>
  </div>
</template>

<script>
import { PDF_WORKER_SRC, ACCEPT_IMAGE_TYPES, ACCEPT_PDF_TYPE, MAX_FILE_SIZE } from '@/config/constants'

/**
 * 上传区域组件
 *
 * @event update:files - 文件列表变化时触发
 * @event error - 错误提示
 */
export default {
  name: 'UploadZone',
  props: {
    /**
     * 最大文件大小（字节），默认 10MB
     */
    maxFileSize: {
      type: Number,
      default: MAX_FILE_SIZE
    },
    /**
     * 允许的文件类型
     */
    acceptTypes: {
      type: Array,
      default: () => [...ACCEPT_IMAGE_TYPES, ACCEPT_PDF_TYPE]
    }
  },
  emits: ['update:files', 'error'],
  data() {
    return {
      files: [],
      isDragOver: false
    }
  },
  beforeUnmount() {
    this.clearAll()
  },
  methods: {
    triggerFileInput() {
      this.$refs.fileInput?.click()
    },

    handleFileSelect(event) {
      const selectedFiles = Array.from(event.target.files || [])
      this.addFiles(selectedFiles)
      event.target.value = ''
    },

    handleDrop(event) {
      this.isDragOver = false
      const droppedFiles = Array.from(event.dataTransfer.files || [])
      this.addFiles(droppedFiles)
    },

    addFiles(newFiles) {
      const validFiles = []
      
      for (const file of newFiles) {
        // 检查文件类型
        if (!this.acceptTypes.includes(file.type)) {
          this.$emit('error', `不支持的文件类型：${file.name}`)
          continue
        }
        
        // 检查文件大小
        if (file.size > this.maxFileSize) {
          this.$emit('error', `文件过大：${file.name}（最大 ${this.formatFileSize(this.maxFileSize)}）`)
          continue
        }
        
        const fileData = {
          file,
          name: file.name,
          size: file.size,
          type: file.type
        }
        
        // 生成预览
        if (file.type.startsWith('image/')) {
          fileData.preview = URL.createObjectURL(file)
        } else if (file.type === 'application/pdf') {
          // PDF 生成缩略图
          this.generatePdfThumbnail(file).then(thumbnail => {
            fileData.preview = thumbnail
            // 触发更新
            this.$emit('update:files', this.files.map(f => f.file))
          }).catch(() => {
            fileData.preview = null
          })
        }
        
        validFiles.push(fileData)
      }

      if (validFiles.length === 0 && newFiles.length > 0) {
        return
      }

      this.files.push(...validFiles)
      console.log('[UploadZone] files 更新:', this.files)
      console.log('[UploadZone] emit update:files:', this.files.map(f => f.file))
      this.$emit('update:files', this.files.map(f => f.file))
    },

    /**
     * 生成 PDF 第一页缩略图
     */
    async generatePdfThumbnail(file) {
      const pdfjsLib = await import('pdfjs-dist')
      pdfjsLib.GlobalWorkerOptions.workerSrc = PDF_WORKER_SRC
      
      const arrayBuffer = await file.arrayBuffer()
      const pdf = await pdfjsLib.getDocument({ data: arrayBuffer }).promise
      const page = await pdf.getPage(1)
      
      const scale = 0.5
      const viewport = page.getViewport({ scale })
      
      const canvas = document.createElement('canvas')
      const context = canvas.getContext('2d')
      canvas.height = viewport.height
      canvas.width = viewport.width
      
      await page.render({
        canvasContext: context,
        viewport: viewport
      }).promise
      
      return canvas.toDataURL('image/jpeg', 0.8)
    },

    removeFile(index) {
      const removed = this.files[index]
      if (removed?.preview) {
        URL.revokeObjectURL(removed.preview)
      }
      this.files.splice(index, 1)
      this.$emit('update:files', this.files.map(f => f.file))
    },

    clearAll() {
      this.files.forEach(f => {
        if (f?.preview) URL.revokeObjectURL(f.preview)
      })
      this.files = []
      this.$emit('update:files', [])
    },

    formatFileSize(bytes) {
      if (bytes === 0) return '0 B'
      const k = 1024
      const sizes = ['B', 'KB', 'MB', 'GB']
      const i = Math.floor(Math.log(bytes) / Math.log(k))
      return Math.round(bytes / Math.pow(k, i) * 100) / 100 + ' ' + sizes[i]
    }
  }
}
</script>

<style scoped>
.upload-zone {
  position: relative;
  border: 3px dashed #667eea;
  border-radius: 12px;
  padding: 1.5rem;
  transition: all 0.3s;
  background: white;
}

.upload-zone:hover,
.upload-placeholder.drag-over {
  border-color: #764ba2;
  background: #f8f9ff;
}

.file-input {
  position: absolute;
  width: 100%;
  height: 100%;
  top: 0;
  left: 0;
  opacity: 0;
  cursor: pointer;
}

.upload-placeholder {
  text-align: center;
  padding: 2rem 1rem;
  color: #667eea;
  cursor: pointer;
}

.upload-icon {
  font-size: 4rem;
  margin-bottom: 1rem;
  opacity: 0.8;
}

.upload-text {
  font-size: 1rem;
  margin-bottom: 0.5rem;
  color: #333;
}

.upload-hint {
  font-size: 0.85rem;
  color: #999;
}

.file-list {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.file-item {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.75rem;
  background: #f8f9ff;
  border-radius: 8px;
  transition: background 0.2s;
}

.file-item:hover {
  background: #f0f2ff;
}

.file-preview {
  width: 48px;
  height: 48px;
  border-radius: 6px;
  overflow: hidden;
  flex-shrink: 0;
}

.file-preview img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.file-info {
  width: 48px;
  height: 48px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: white;
  border-radius: 6px;
  flex-shrink: 0;
}

.file-type-icon {
  font-size: 1.5rem;
}

.file-details {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.file-name {
  font-size: 0.9rem;
  color: #333;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.file-size {
  font-size: 0.75rem;
  color: #999;
}

.remove-btn {
  background: #ff4757;
  color: white;
  border: none;
  width: 28px;
  height: 28px;
  border-radius: 50%;
  cursor: pointer;
  font-size: 1.25rem;
  line-height: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  transition: transform 0.2s;
}

.remove-btn:hover {
  transform: scale(1.1);
}

.upload-actions {
  display: flex;
  gap: 0.5rem;
  margin-top: 1rem;
  padding-top: 1rem;
  border-top: 1px solid #e0e0e0;
}

.btn-add,
.btn-clear {
  padding: 0.5rem 1rem;
  border-radius: 6px;
  font-size: 0.9rem;
  cursor: pointer;
  transition: all 0.2s;
  display: flex;
  align-items: center;
  gap: 0.25rem;
}

.btn-add {
  background: #f0f2ff;
  color: #667eea;
  border: 2px solid #667eea;
}

.btn-add:hover {
  background: #667eea;
  color: white;
}

.btn-clear {
  background: #fff5f5;
  color: #ff4757;
  border: 2px solid #ff4757;
}

.btn-clear:hover {
  background: #ff4757;
  color: white;
}

@media (max-width: 600px) {
  .upload-actions {
    flex-direction: column;
  }

  .btn-add,
  .btn-clear {
    width: 100%;
    justify-content: center;
  }
}
</style>
