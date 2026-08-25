<script setup lang="ts">
import { computed, onMounted, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useLauncherStore } from '@/stores/launcher'
import { api } from '@/api'

const route = useRoute()
const router = useRouter()
const { t, locale } = useI18n()
const store = useLauncherStore()
const isTauri = api.isTauri

onMounted(async () => {
  await store.init()
  locale.value = store.settings.locale || 'zh-CN'
})

watch(
  () => store.settings.locale,
  (v) => {
    if (v) locale.value = v
  },
)

const selectedKeys = computed(() => {
  const name = route.name
  if (name === 'versions' || name === 'plugins' || name === 'settings') return [name as string]
  return ['home']
})

function onMenuSelect(key: string) {
  router.push({ name: key })
}
</script>

<template>
  <a-layout class="app-shell">
    <a-layout-header class="app-header">
      <div class="app-brand">
        <span class="app-logo">⚡</span>
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
        <a-menu-item key="versions">{{ t('nav.versions') }}</a-menu-item>
        <a-menu-item key="plugins">{{ t('nav.plugins') }}</a-menu-item>
        <a-menu-item key="settings">{{ t('nav.settings') }}</a-menu-item>
      </a-menu>
    </a-layout-header>
    <a-layout-content>
      <router-view />
    </a-layout-content>
  </a-layout>
</template>

<style lang="scss" scoped>
.app-shell {
  height: 100%;
}

.app-header {
  display: flex;
  align-items: center;
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

  .app-logo {
    font-size: 20px;
  }

  .app-title {
    font-size: 16px;
    font-weight: 600;
  }
}

.app-menu {
  flex: 1;
  border-bottom: none;

  :deep(.arco-menu-inner) {
    border-bottom: none;
  }
}
</style>
