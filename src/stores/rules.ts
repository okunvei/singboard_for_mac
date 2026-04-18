import { ref, computed } from 'vue'
import { fetchRules } from '@/api'
import type { Rule } from '@/types'

const rules = ref<Rule[]>([])
const loading = ref(false)
const filterText = ref('')

const filteredRules = computed(() => {
  if (!filterText.value) return rules.value
  const q = filterText.value.toLowerCase()
  return rules.value.filter(
    (r) =>
      r.payload.toLowerCase().includes(q) ||
      r.proxy.toLowerCase().includes(q) ||
      r.type.toLowerCase().includes(q),
  )
})

export function useRulesStore() {
  async function loadRules() {
    loading.value = true
    try {
      const { data } = await fetchRules()
      rules.value = data.rules
    } catch {
      rules.value = []
    } finally {
      loading.value = false
    }
  }

  return {
    rules,
    filteredRules,
    loading,
    filterText,
    loadRules,
  }
}
