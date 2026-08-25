<script setup lang="ts">
import { computed } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Message } from '@arco-design/web-vue'
import { api } from '@/api'
import { useLauncherStore } from '@/stores/launcher'
import type { InstanceState } from '@/api/types'

const router = useRouter()
const { t } = useI18n()
const store = useLauncherStore()

const columns = computed(() => [
  { title: t('instances.table.name'), dataIndex: 'name', width: 180 },
  { title: t('instances.table.version'), slotName: 'version', width: 140 },
  { title: t('instances.table.home'), slotName: 'home', width: 180 },
  { title: t('instances.table.profile'), slotName: 'profile', width: 120 },
  { title: t('instances.table.status'), slotName: 'status' },
  { title: t('instances.table.actions'), slotName: 'actions', width: 150, align: 'center' as const },
])

function stateColor(state: InstanceState): string {
  switch (state) {
    case 'running':
      return 'green'
    case 'starting':
      return 'orange'
    case 'exited':
      return 'red'
    default:
      return 'gray'
  }
}

async function onDelete(id: string, name: string) {
  try {
    await api.deleteInstance(id)
    await store.refreshInstances()
    Message.success(t('instances.deleted'))
  } catch (e) {
    Message.error(String(e))
  }
}

function copyUrl(url: string) {
  navigator.clipboard?.writeText(url)
  Message.success(t('common.copied'))
}

async function onOpenWindow(id: string) {
  try {
    await api.openInstanceWindow(id)
  } catch (e) {
    Message.error(String(e))
  }
}
</script>

<template>
  <div class="dl-page">
    <div class="dl-card">
      <div class="dl-card-title">
        <h3>{{ t('instances.title') }}</h3>
        <div class="dl-toolbar">
          <a-button type="primary" @click="router.push({ name: 'download' })">
            {{ t('instances.newInstance') }}
          </a-button>
        </div>
      </div>

      <a-table :columns="columns" :data="store.instances" :pagination="false" row-key="id">
        <template #version="{ record }">
          {{ store.versionById(record.version_id)?.version ?? record.version_id }}
        </template>
        <template #home="{ record }">
          <a-tooltip :content="store.homeById(record.home_id)?.path">
            <span>{{ store.homeById(record.home_id)?.name ?? record.home_id }}</span>
          </a-tooltip>
        </template>
        <template #profile="{ record }">
          {{ record.last_profile ?? record.default_profile ?? '—' }}
        </template>
        <template #status="{ record }">
          <a-tag :color="stateColor(store.statusOf(record.id).state)">
            {{ t(`home.status.${store.statusOf(record.id).state}`) }}
          </a-tag>
          <template v-if="store.statusOf(record.id).url">
            <a-link class="status-url" @click="onOpenWindow(record.id)">
              {{ store.statusOf(record.id).url }}
            </a-link>
            <a-button size="mini" type="text" @click="copyUrl(store.statusOf(record.id).url!)">
              {{ t('common.copy') }}
            </a-button>
          </template>
        </template>
        <template #actions="{ record }">
          <a-space>
            <a-button
              size="small"
              @click="router.push({ name: 'instance-edit', params: { id: record.id } })"
            >
              {{ t('instances.table.edit') }}
            </a-button>
            <a-popconfirm
              :content="t('instances.confirmDelete', { name: record.name })"
              @ok="onDelete(record.id, record.name)"
            >
              <a-button size="small" status="danger">{{ t('instances.table.delete') }}</a-button>
            </a-popconfirm>
          </a-space>
        </template>
        <template #empty>
          <a-empty :description="t('instances.emptyDesc')">
            <template #image>
              <div class="empty-title">{{ t('instances.emptyTitle') }}</div>
            </template>
            <a-button type="primary" @click="router.push({ name: 'download' })">
              {{ t('instances.newInstance') }}
            </a-button>
          </a-empty>
        </template>
      </a-table>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.status-url {
  margin-left: 8px;
  font-size: 12px;
}

.empty-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--color-text-1);
}
</style>
