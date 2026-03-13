import { createPinia } from 'pinia'

import { useApiStore } from './api'
import { useUploadStore, useChatStore } from './upload'

const pinia = createPinia()

export { useApiStore, useUploadStore, useChatStore }
export default pinia
