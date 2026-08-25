<script setup lang="ts">
import { computed, onMounted, onUnmounted, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useLauncherStore } from '@/stores/launcher'
import type { ThemeMode } from '@/api/types'
import { api } from '@/api'

const route = useRoute()
const router = useRouter()
const { t, locale } = useI18n()
const store = useLauncherStore()
const isTauri = api.isTauri

// --- Theme: light / dark / follow system -------------------------------------

const themeMedia = window.matchMedia('(prefers-color-scheme: dark)')

/** Applies the effective Arco theme to <body> (body[arco-theme='dark']). */
function applyTheme(mode: ThemeMode) {
  const dark = mode === 'dark' || (mode === 'system' && themeMedia.matches)
  if (dark) {
    document.body.setAttribute('arco-theme', 'dark')
  } else {
    document.body.removeAttribute('arco-theme')
  }
}

function onSystemThemeChange() {
  if (store.settings.theme === 'system') applyTheme('system')
}

onMounted(async () => {
  await store.init()
  locale.value = store.settings.locale || 'zh-CN'
  // Apply the persisted theme early (before init resolves the settings may
  // still be defaults; the watch below re-applies on any change).
  applyTheme(store.settings.theme || 'system')
  themeMedia.addEventListener('change', onSystemThemeChange)
  // If Node.js is missing, guide the user to install it before anything else.
  if (!store.runtime?.node?.installed && route.name !== 'setup') {
    router.push({ name: 'setup' })
  }
})

onUnmounted(() => {
  themeMedia.removeEventListener('change', onSystemThemeChange)
})

watch(
  () => store.settings.locale,
  (v) => {
    if (v) locale.value = v
  },
)

watch(
  () => store.settings.theme,
  (v) => {
    if (v) applyTheme(v)
  },
)

const selectedKeys = computed(() => {
  const name = route.name as string
  if (name === 'download' || name?.startsWith('download-')) return ['download']
  if (name === 'settings') return ['settings']
  if (name === 'home') return ['home']
  return []
})

const onTasksPage = computed(() => route.name === 'tasks')

function onFabClick() {
  if (onTasksPage.value) {
    router.back()
  } else {
    router.push({ name: 'tasks' })
  }
}

function onMenuSelect(key: string) {
  router.push({ name: key })
}

const appWindow = (() => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return (window as any)?.__TAURI_INTERNALS__ ? loadWindowApi() : null
})()

async function loadWindowApi() {
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  return getCurrentWindow()
}

async function minimize() {
  const w = await appWindow
  w?.minimize()
}

async function close() {
  const w = await appWindow
  // Delegate to the native close request: the Rust CloseRequested handler
  // honors minimize_to_tray (hide) or lets the window close for real.
  await w?.close()
}

// Manual drag: native data-tauri-drag-region only works on elements carrying
// the attribute, which leaves the menu area in the middle undraggable.
async function onHeaderMouseDown(e: MouseEvent) {
  if (!isTauri || e.button !== 0) return
  const el = e.target as HTMLElement | null
  if (el?.closest('.window-controls, .arco-menu-item, a, button, input, [data-no-drag]')) return
  const w = await appWindow
  w?.startDragging()
}
</script>

<template>
  <a-layout class="app-shell">
    <a-layout-header class="app-header" @mousedown="onHeaderMouseDown">
      <!-- Brand; dragging is handled manually via onHeaderMouseDown. -->
      <div class="app-brand">
        <img src="@/assets/launcher-icon.png" class="app-logo" alt="" />
        <span class="app-title">{{ t('app.title') }}</span>
        <a-tag v-if="!isTauri" size="small" color="orange">{{ t('app.mockBadge') }}</a-tag>
      </div>
      <a-menu
        mode="horizontal"
        :selected-keys="selectedKeys"
        class="app-menu"
        @menu-item-click="onMenuSelect"
      >
        <a-menu-item key="home">{{ t('nav.home') }}</a-menu-item>
        <a-menu-item key="download">{{ t('nav.download') }}</a-menu-item>
        <a-menu-item key="settings">{{ t('nav.settings') }}</a-menu-item>
      </a-menu>
      <div class="window-controls">
        <button class="wc-btn" title="最小化" @click="minimize">
          <svg viewBox="0 0 12 12" width="12" height="12"><line x1="1" y1="6" x2="11" y2="6" stroke="currentColor" stroke-width="1.4"/></svg>
        </button>
        <button class="wc-btn wc-close" title="关闭" @click="close">
          <svg viewBox="0 0 12 12" width="12" height="12"><path d="M1 1 L11 11 M11 1 L1 11" stroke="currentColor" stroke-width="1.4"/></svg>
        </button>
      </div>
    </a-layout-header>
    <a-layout-content class="app-content">
      <a-scrollbar
        type="track"
        outer-style="height: 100%"
        style="height: 100%; overflow-y: auto"
      >
        <router-view />
      </a-scrollbar>
    </a-layout-content>

    <!-- Floating task manager entry (bottom-right); becomes a back button on the tasks page. -->
    <div class="task-fab" @click="onFabClick">
      <a-badge v-if="!onTasksPage" :count="store.runningTaskCount" :dot="store.runningTaskCount > 0">
        <span class="task-fab-icon">⏱</span>
      </a-badge>
      <span v-else class="task-fab-icon">←</span>
      <span class="task-fab-text">{{ onTasksPage ? t('download.back') : t('tasks.fab') }}</span>
    </div>
  </a-layout>
</template>

<style lang="scss" scoped>
.app-shell {
  height: 100%;
}

.app-content {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.app-header {
  display: flex;
  align-items: center;
  flex-shrink: 0;
  height: var(--dl-header-height);
  padding: 0 20px;
  background: var(--color-bg-2);
  border-bottom: 1px solid var(--color-border-2);
}

.app-brand {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-right: 32px;
  white-space: nowrap;
  height: 100%;
  cursor: default;

  .app-logo {
    width: 24px;
    height: 24px;
    border-radius: 5px;
    object-fit: cover;
  }

  .app-title {
    font-size: 16px;
    font-weight: 600;
  }
}

.window-controls {
  display: flex;
  align-items: center;
  gap: 2px;
  margin-right: -12px;
}

.wc-btn {
  width: 42px;
  height: var(--dl-header-height);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: transparent;
  color: var(--color-text-2);
  cursor: pointer;

  &:hover {
    background: var(--color-fill-2);
    color: var(--color-text-1);
  }
}

.wc-close:hover {
  background: #e81123;
  color: #fff;
}

.app-menu {
  flex: 1;
  border-bottom: none;

  :deep(.arco-menu-inner) {
    border-bottom: none;
  }
}

.task-fab {
  position: fixed;
  right: 24px;
  bottom: 24px;
  z-index: 100;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 18px;
  background: var(--color-bg-2);
  border: 1px solid var(--color-border-2);
  border-radius: 24px;
  box-shadow: 0 4px 16px rgb(0 0 0 / 12%);
  cursor: pointer;
  user-select: none;
  transition: transform 0.15s, box-shadow 0.15s;

  &:hover {
    transform: translateY(-2px);
    box-shadow: 0 8px 24px rgb(22 93 255 / 20%);
  }
}

.task-fab-icon {
  font-size: 18px;
}

.task-fab-text {
  font-size: 13px;
  font-weight: 600;
  color: var(--color-text-1);
}
</style>
