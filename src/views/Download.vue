<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useLauncherStore } from '@/stores/launcher'

const route = useRoute()
const router = useRouter()
const { t } = useI18n()
const store = useLauncherStore()

const selectedKeys = computed(() => {
  const name = route.name as string
  if (name === 'download-plugins') return ['plugins']
  return ['create']
})

const onCreatePage = computed(() => selectedKeys.value[0] === 'create')

function onMenuSelect(key: string) {
  router.push({ name: key === 'plugins' ? 'download-plugins' : 'download-create' })
}

function onRefreshVersions() {
  store.refreshRemoteVersions()
}
</script>

<template>
  <div class="download-page">
    <aside class="download-sidebar">
      <a-menu :selected-keys="selectedKeys" @menu-item-click="onMenuSelect">
        <a-menu-item key="create">
          <span class="menu-line">
            {{ t('download.createInstance') }}
            <a-button
              v-if="onCreatePage"
              type="text"
              size="mini"
              class="refresh-btn"
              :loading="store.remoteLoading"
              @click.stop="onRefreshVersions"
            >
              ⟳
            </a-button>
          </span>
        </a-menu-item>
        <a-menu-item key="plugins">{{ t('download.plugins') }}</a-menu-item>
      </a-menu>
    </aside>
    <section class="download-content">
      <router-view />
    </section>
  </div>
</template>

<style lang="scss" scoped>
.download-page {
  display: flex;
  height: calc(100vh - var(--dl-header-height));
}

.download-sidebar {
  width: 200px;
  flex-shrink: 0;
  background: var(--color-bg-2);
  border-right: 1px solid var(--color-border-2);

  :deep(.arco-menu) {
    height: 100%;
  }
}

.menu-line {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  width: 100%;
}

.refresh-btn {
  margin-left: auto;
  padding: 0 4px;
  font-size: 14px;
  color: var(--color-text-3);

  &:hover {
    color: rgb(var(--primary-6));
  }
}

.download-content {
  flex: 1;
  overflow-y: auto;
  padding: 20px 24px 80px;
}
</style>
