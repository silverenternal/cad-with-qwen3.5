<template>
  <section class="loading-section">
    <div class="loading-spinner">⏳</div>
    <p class="loading-text">{{ text }}</p>
    <div v-if="progress > 0" class="progress-bar">
      <div class="progress-fill" :style="{ width: progress + '%' }"></div>
    </div>
  </section>
</template>

<script>
/**
 * 加载状态组件
 */
export default {
  name: 'LoadingSection',
  props: {
    text: {
      type: String,
      required: true
    },
    progress: {
      type: Number,
      default: 0
    }
  }
}
</script>

<style scoped>
.loading-section {
  background: white;
  border-radius: 16px;
  padding: 2rem;
  text-align: center;
  margin-bottom: 1.5rem;
  box-shadow: 0 10px 40px rgba(0, 0, 0, 0.1);
}

.loading-spinner {
  font-size: 3rem;
  animation: pulse 1.5s infinite;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}

.loading-text {
  margin-top: 1rem;
  color: #333;
  font-size: 1rem;
}

.progress-bar {
  width: 100%;
  height: 8px;
  background: #f0f0f0;
  border-radius: 4px;
  margin-top: 1rem;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: linear-gradient(90deg, #667eea 0%, #764ba2 100%);
  transition: width 0.3s ease;
}
</style>
