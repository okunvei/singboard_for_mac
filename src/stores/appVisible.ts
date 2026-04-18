import { ref } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'

export const appVisible = ref(true)

getCurrentWindow().listen<boolean>('window-visibility', (event) => {
  appVisible.value = event.payload
})
