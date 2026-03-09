<script setup lang="ts">
import { computed, ref, onMounted, onBeforeUnmount, watch, nextTick } from 'vue'
import { useToastStore } from '@/stores/toast'
import { readSingboxConfig, writeSingboxConfig, validateSingboxConfigContent } from '@/bridge/config'
import { EditorView, keymap, lineNumbers, highlightActiveLineGutter, highlightSpecialChars, drawSelection, highlightActiveLine } from '@codemirror/view'
import { EditorState } from '@codemirror/state'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { json } from '@codemirror/lang-json'
import { syntaxHighlighting, defaultHighlightStyle, bracketMatching, foldGutter, foldKeymap, indentOnInput, HighlightStyle } from '@codemirror/language'
import { searchKeymap, highlightSelectionMatches } from '@codemirror/search'
import { autocompletion, completionKeymap, closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete'
import { lintGutter } from '@codemirror/lint'
import { tags } from '@lezer/highlight'

const props = defineProps<{
  configPath: string
  singboxPath: string
  workingDir: string
}>()

const { pushToast } = useToastStore()

const editorContainer = ref<HTMLElement>()
let editorView: EditorView | null = null
const loading = ref(false)
const saving = ref(false)
const validating = ref(false)
const hasChanges = ref(false)
const editorMode = ref<'whole' | 'module'>('whole')

const preferredModuleOrder = [
  'log',
  'dns',
  'ntp',
  'certificate',
  'endpoints',
  'inbounds',
  'outbounds',
  'route',
  'services',
  'experimental',
] as const

const moduleDefaults: Record<string, unknown> = {
  log: {},
  dns: {},
  ntp: {},
  certificate: {},
  endpoints: [],
  inbounds: [],
  outbounds: [],
  route: {},
  services: [],
  experimental: {},
}

const activeModule = ref<string>('log')
const fullConfigObject = ref<Record<string, unknown> | null>(null)
const savedRawContent = ref('')
const savedFullNormalized = ref('')
const moduleItems = computed<Array<{ key: string; label: string }>>(() => {
  const keys = new Set<string>()
  const dynamicKeys = fullConfigObject.value ? Object.keys(fullConfigObject.value) : []

  for (const key of preferredModuleOrder) {
    if (dynamicKeys.includes(key)) keys.add(key)
  }
  for (const key of dynamicKeys) {
    keys.add(key)
  }

  if (keys.size === 0) {
    for (const key of preferredModuleOrder) keys.add(key)
  }

  return Array.from(keys).map((key) => ({ key, label: key }))
})

const editorTheme = EditorView.theme({
  '&': {
    height: '100%',
    fontSize: '13px',
  },
  '.cm-scroller': {
    fontFamily: "'Cascadia Code', 'Fira Code', 'JetBrains Mono', Consolas, monospace",
    overflow: 'auto',
  },
  '.cm-content': {
    caretColor: 'oklch(var(--bc))',
  },
  '.cm-cursor': {
    borderLeftColor: 'oklch(var(--bc))',
  },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground': {
    backgroundColor: 'oklch(var(--p) / 0.2) !important',
  },
  '.cm-activeLine': {
    backgroundColor: 'oklch(var(--bc) / 0.05)',
  },
  '.cm-activeLineGutter': {
    backgroundColor: 'oklch(var(--bc) / 0.08)',
  },
  '.cm-gutters': {
    backgroundColor: 'oklch(var(--b2))',
    color: 'oklch(var(--bc) / 0.4)',
    borderRight: '1px solid oklch(var(--b3))',
  },
  '.cm-foldGutter': {
    width: '12px',
  },
  '.cm-tooltip': {
    backgroundColor: 'oklch(var(--b2))',
    border: '1px solid oklch(var(--b3))',
    color: 'oklch(var(--bc))',
  },
  '.cm-panels': {
    backgroundColor: 'oklch(var(--b2))',
    color: 'oklch(var(--bc))',
  },
  '.cm-panel.cm-search': {
    backgroundColor: 'oklch(var(--b2))',
  },
  '.cm-panel.cm-search input': {
    backgroundColor: 'oklch(var(--b1))',
    color: 'oklch(var(--bc))',
    border: '1px solid oklch(var(--b3))',
  },
  '.cm-panel.cm-search button': {
    backgroundColor: 'oklch(var(--b3))',
    color: 'oklch(var(--bc))',
  },
  '.cm-matchingBracket': {
    backgroundColor: 'oklch(var(--p) / 0.15)',
    outline: '1px solid oklch(var(--p) / 0.4)',
  },
})

const highlightColors = HighlightStyle.define([
  { tag: tags.string, color: 'oklch(var(--su, 0.6 0.15 160))' },
  { tag: tags.number, color: 'oklch(var(--wa, 0.7 0.15 60))' },
  { tag: tags.bool, color: 'oklch(var(--er, 0.65 0.2 25))' },
  { tag: tags.null, color: 'oklch(var(--bc) / 0.5)' },
  { tag: tags.propertyName, color: 'oklch(var(--p, 0.6 0.2 270))' },
  { tag: tags.punctuation, color: 'oklch(var(--bc) / 0.6)' },
])

function createEditor() {
  if (!editorContainer.value) return

  const state = EditorState.create({
    doc: '',
    extensions: [
      lineNumbers(),
      highlightActiveLineGutter(),
      highlightSpecialChars(),
      history(),
      foldGutter(),
      drawSelection(),
      indentOnInput(),
      bracketMatching(),
      closeBrackets(),
      autocompletion(),
      highlightActiveLine(),
      highlightSelectionMatches(),
      lintGutter(),
      json(),
      editorTheme,
      syntaxHighlighting(highlightColors),
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      keymap.of([
        ...closeBracketsKeymap,
        ...defaultKeymap,
        ...searchKeymap,
        ...historyKeymap,
        ...foldKeymap,
        ...completionKeymap,
        indentWithTab,
        { key: 'Mod-s', run: () => { void handleSave(); return true } },
      ]),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          recomputeDirtyState()
        }
      }),
    ],
  })

  editorView = new EditorView({
    state,
    parent: editorContainer.value,
  })
}

function setEditorContent(content: string, keepChangeState = false) {
  if (!editorView) return
  editorView.dispatch({
    changes: { from: 0, to: editorView.state.doc.length, insert: content },
  })
  if (!keepChangeState) {
    hasChanges.value = false
  }
}

function getEditorContent(): string {
  return editorView?.state.doc.toString() ?? ''
}

function normalizeRootObject(value: unknown): string | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return null
  }
  return JSON.stringify(value, null, 2)
}

function cloneDefaultValue(key: string): unknown {
  const val = moduleDefaults[key]
  if (val === undefined) return {}
  return Array.isArray(val) ? [] : {}
}

function ensureStructuredConfigFromEditor(showError = true): boolean {
  try {
    const parsed = JSON.parse(getEditorContent())
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      if (showError) {
        pushToast({ message: '配置根节点必须是 JSON 对象', type: 'error' })
      }
      return false
    }
    fullConfigObject.value = parsed as Record<string, unknown>
    return true
  } catch (e: any) {
    if (showError) {
      pushToast({ message: '当前内容不是合法 JSON，无法进入分模块编辑: ' + e.message, type: 'error' })
    }
    return false
  }
}

function ensureModuleType(key: string, value: unknown): boolean {
  if (!(key in moduleDefaults)) {
    return true
  }
  const shouldArray = Array.isArray(moduleDefaults[key])
  if (shouldArray) return Array.isArray(value)
  return !!value && typeof value === 'object' && !Array.isArray(value)
}

function applyEditorChangesToState(showError = true): boolean {
  const content = getEditorContent()
  if (editorMode.value === 'whole') {
    try {
      const parsed = JSON.parse(content)
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
        if (showError) {
          pushToast({ message: '配置根节点必须是 JSON 对象', type: 'error' })
        }
        return false
      }
      fullConfigObject.value = parsed as Record<string, unknown>
      return true
    } catch (e: any) {
      if (showError) {
        pushToast({ message: 'JSON 语法错误: ' + e.message, type: 'error' })
      }
      return false
    }
  }

  if (!fullConfigObject.value) {
    fullConfigObject.value = {}
  }
  try {
    const parsed = JSON.parse(content)
    const key = activeModule.value
    if (!ensureModuleType(key, parsed)) {
      if (showError) {
        const expected = Array.isArray(moduleDefaults[key]) ? '数组' : '对象'
        pushToast({ message: `${key} 模块必须是 JSON ${expected}`, type: 'error' })
      }
      return false
    }
    fullConfigObject.value[key] = parsed
    return true
  } catch (e: any) {
    if (showError) {
      pushToast({ message: 'JSON 语法错误: ' + e.message, type: 'error' })
    }
    return false
  }
}

function draftFullConfigForDirtyCheck(): Record<string, unknown> | null {
  if (editorMode.value === 'whole') {
    try {
      const parsed = JSON.parse(getEditorContent())
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
        return null
      }
      return parsed as Record<string, unknown>
    } catch {
      return null
    }
  }

  if (!fullConfigObject.value) return null
  try {
    const parsed = JSON.parse(getEditorContent())
    if (!ensureModuleType(activeModule.value, parsed)) {
      return null
    }
    return {
      ...fullConfigObject.value,
      [activeModule.value]: parsed,
    }
  } catch {
    return null
  }
}

function recomputeDirtyState() {
  const current = getEditorContent()
  if (editorMode.value === 'whole' && !savedFullNormalized.value) {
    hasChanges.value = current !== savedRawContent.value
    return
  }

  const draft = draftFullConfigForDirtyCheck()
  if (!draft) {
    hasChanges.value = true
    return
  }
  hasChanges.value = JSON.stringify(draft, null, 2) !== savedFullNormalized.value
}

function renderModuleContent(keepChangeState = false) {
  if (!fullConfigObject.value) return
  const key = activeModule.value
  let value = fullConfigObject.value[key]
  if (value === undefined) {
    value = cloneDefaultValue(key)
    fullConfigObject.value[key] = value
  }
  setEditorContent(JSON.stringify(value, null, 2), keepChangeState)
}

function switchMode(mode: 'whole' | 'module') {
  if (mode === editorMode.value) return

  if (!applyEditorChangesToState()) return

  const keepChanges = hasChanges.value
  if (mode === 'module') {
    if (!fullConfigObject.value && !ensureStructuredConfigFromEditor()) return
    if (!moduleItems.value.some((item) => item.key === activeModule.value)) {
      activeModule.value = moduleItems.value[0]?.key ?? 'log'
    }
    editorMode.value = 'module'
    renderModuleContent(keepChanges)
    return
  }

  editorMode.value = 'whole'
  if (fullConfigObject.value) {
    setEditorContent(JSON.stringify(fullConfigObject.value, null, 2), keepChanges)
  }
}

function switchModule(next: string) {
  if (next === activeModule.value) return
  if (editorMode.value !== 'module') {
    activeModule.value = next
    return
  }
  if (!applyEditorChangesToState()) return
  const keepChanges = hasChanges.value
  activeModule.value = next
  renderModuleContent(keepChanges)
}

async function loadConfig() {
  if (!props.configPath) {
    pushToast({ message: '请先在设置中配置配置文件路径', type: 'error' })
    return
  }
  loading.value = true
  try {
    const content = await readSingboxConfig(props.configPath)
    savedRawContent.value = content
    setEditorContent(content)
    try {
      const parsed = JSON.parse(content)
      fullConfigObject.value =
        parsed && typeof parsed === 'object' && !Array.isArray(parsed)
          ? (parsed as Record<string, unknown>)
          : null
      savedFullNormalized.value = normalizeRootObject(fullConfigObject.value) ?? ''
    } catch {
      fullConfigObject.value = null
      savedFullNormalized.value = ''
    }
    recomputeDirtyState()
  } catch (e: any) {
    pushToast({ message: '读取配置文件失败: ' + (e?.message || e), type: 'error' })
  } finally {
    loading.value = false
  }
}

async function handleSave() {
  if (!props.configPath) return
  saving.value = true
  try {
    if (!applyEditorChangesToState()) {
      saving.value = false
      return
    }
    const content =
      editorMode.value === 'whole'
        ? getEditorContent()
        : JSON.stringify(fullConfigObject.value ?? {}, null, 2)
    await writeSingboxConfig(props.configPath, content)

    const normalized = normalizeRootObject(fullConfigObject.value)
    if (normalized) {
      savedFullNormalized.value = normalized
      savedRawContent.value = normalized
      if (editorMode.value === 'whole') {
        setEditorContent(normalized)
      } else {
        renderModuleContent()
      }
    } else {
      savedRawContent.value = content
      savedFullNormalized.value = ''
      recomputeDirtyState()
    }
    hasChanges.value = false
    pushToast({ message: '配置文件已保存', type: 'info' })
  } catch (e: any) {
    pushToast({ message: '保存失败: ' + (e?.message || e), type: 'error' })
  } finally {
    saving.value = false
  }
}

async function handleValidate() {
  if (!props.singboxPath || !props.configPath) {
    pushToast({ message: '请先在设置中配置 sing-box 路径', type: 'error' })
    return
  }
  if (!applyEditorChangesToState()) {
    return
  }
  validating.value = true
  try {
    const content =
      editorMode.value === 'whole'
        ? getEditorContent()
        : JSON.stringify(fullConfigObject.value ?? {}, null, 2)
    await validateSingboxConfigContent(
      props.singboxPath,
      props.configPath,
      content,
      props.workingDir,
    )
    pushToast({ message: '配置文件校验通过', type: 'info' })
  } catch (e: any) {
    pushToast({ message: '校验失败:\n' + (e?.message || e), type: 'error' }, 8000)
  } finally {
    validating.value = false
  }
}

function handleFormat() {
  try {
    if (!applyEditorChangesToState()) return
    const formatted =
      editorMode.value === 'whole'
        ? JSON.stringify(fullConfigObject.value ?? {}, null, 2)
        : JSON.stringify((fullConfigObject.value ?? {})[activeModule.value], null, 2)
    setEditorContent(formatted)
    recomputeDirtyState()
    pushToast({ message: '已格式化', type: 'info' })
  } catch {}
}

onMounted(() => {
  nextTick(() => {
    createEditor()
    loadConfig()
  })
})

onBeforeUnmount(() => {
  editorView?.destroy()
  editorView = null
})

watch(() => props.configPath, (newPath, oldPath) => {
  if (newPath && newPath !== oldPath) {
    loadConfig()
  }
})
</script>

<template>
  <div class="bg-base-200 rounded-lg p-4 flex flex-col h-full min-h-0">
    <div class="flex items-center justify-between gap-2 flex-wrap shrink-0">
      <div class="flex items-center gap-2 min-w-0">
        <h2 class="font-semibold text-sm shrink-0">配置编辑</h2>
        <span v-if="props.configPath" class="text-xs text-base-content/50 truncate max-w-md" :title="props.configPath">
          {{ props.configPath }}
        </span>
        <span v-if="hasChanges" class="badge badge-xs badge-warning">未保存</span>
      </div>
      <div class="flex items-center gap-2 flex-wrap">
        <div class="join">
          <button
            class="btn btn-xs join-item"
            :class="editorMode === 'whole' ? 'btn-primary' : 'btn-ghost'"
            @click="switchMode('whole')"
          >
            整体编辑
          </button>
          <button
            class="btn btn-xs join-item"
            :class="editorMode === 'module' ? 'btn-primary' : 'btn-ghost'"
            @click="switchMode('module')"
          >
            分模块编辑
          </button>
        </div>
        <select
          v-if="editorMode === 'module'"
          class="select select-xs select-bordered w-36"
          :value="activeModule"
          @change="switchModule(($event.target as HTMLSelectElement).value)"
        >
          <option v-for="item in moduleItems" :key="item.key" :value="item.key">
            {{ item.label }}
          </option>
        </select>
      </div>
      <div class="flex items-center gap-1.5">
        <button
          class="btn btn-xs btn-ghost"
          :disabled="loading"
          @click="loadConfig"
          title="重新加载"
        >
          刷新
        </button>
        <button
          class="btn btn-xs btn-ghost"
          @click="handleFormat"
          title="格式化 JSON"
        >
          格式化
        </button>
        <button
          class="btn btn-xs btn-outline"
          :class="{ loading: validating }"
          @click="handleValidate"
          title="使用 sing-box 校验配置"
        >
          校验
        </button>
        <button
          class="btn btn-xs btn-primary"
          :class="{ loading: saving }"
          :disabled="!hasChanges"
          @click="handleSave"
          title="保存 (Ctrl+S)"
        >
          保存
        </button>
      </div>
    </div>

    <div class="relative flex-1 min-h-[360px] rounded-lg border border-base-300 bg-base-100 overflow-hidden mt-3">
      <div
        ref="editorContainer"
        class="h-full w-full overflow-hidden"
      ></div>
      <div
        v-if="loading"
        class="absolute inset-0 flex items-center justify-center bg-base-100/70 text-base-content/60 backdrop-blur-[1px]"
      >
        加载中...
      </div>
    </div>
  </div>
</template>
