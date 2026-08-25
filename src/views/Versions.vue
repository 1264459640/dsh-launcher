<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Message } from '@arco-design/web-vue'
import { api } from '@/api'
import { useLauncherStore } from '@/stores/launcher'
import type { InstallProgress } from '@/api/types'

const { t } = useI18n()
const store = useLauncherStore()

// --- Install modal -----------------------------------------------------------

const modalVisible = ref(false)
const fetching = ref(false)
const remoteVersions = ref<string[]>([])
const pickedVersion = ref<string | undefined>(undefined)
const installing = ref(false)
const progress = ref(0)

const installable = computed(() =>
  remoteVersions.value.filter((v) => !store.versions.some((iv) => iv.version === v)),
)

let unlisten: (() => void) | null = null

onMounted(async () => {
  unlisten = await api.onInstallProgress((p: InstallProgress) => {
    if (p.version !== pickedVersion.value) return
    progress.value = p.percent
    if (p.stage === 'done') {
      installing.value = false
      modalVisible.value = false
      store.refreshVersions()
      Message.success(t('versions.installedOk', { version: p.version }))
    } else if (p.stage === 'error') {
      installing.value = false
      Message.error(t('versions.installFailed', { msg: p.message ?? '' }))
    }
  })
})

onBeforeUnmount(() => unlisten?.())

async function onFetch() {
  fetching.value = true
  try {
    remoteVersions.value = await api.fetchAvailableVersions()
  } catch (e) {
    Message.error(t('common.operationFailed', { msg: String(e) }))
  } finally {
    fetching.value = false
  }
}

async function onInstall() {
  if (!pickedVersion.value) return
  installing.value = true
  progress.value = 0
  try {
    await api.installVersion(pickedVersion.value)
    // In the Tauri backend the command returns after install; mock resolves
    // immediately and completion arrives via the progress event.
    if (api.isTauri) {
      installing.value = false
      modalVisible.value = false
      await store.refreshVersions()
      Message.success(t('versions.installedOk', { version: pickedVersion.value ?? '' }))
    }
  } catch (e) {
    installing.value = false
    Message.error(t('versions.installFailed', { msg: String(e) }))
  }
}

// --- Installed table ----------------------------------------------------------

const columns = computed(() => [
  { title: t('versions.table.version'), dataIndex: 'version', width: 160 },
  { title: t('versions.table.dir'), dataIndex: 'dir', ellipsis: true, tooltip: true },
  { title: t('versions.table.usedBy'), slotName: 'usedBy', width: 130 },
  { title: t('versions.table.actions'), slotName: 'actions', width: 110, align: 'center' as const },
])

function usedByCount(versionId: string) {
  return store.instances.filter((i) => i.version_id === versionId).length
}

async function onRemove(id: string, version: string) {
  try {
    await api.removeVersion(id)
    await store.refreshVersions()
    Message.success(t('versions.deleted', { version }))
  } catch (e) {
    Message.error(String(e))
  }
}
</script>

<template>
  <div class="dl-page">
    <div class="dl-card">
      <div class="dl-card-title">
        <h3>{{ t('versions.installed') }}</h3>
        <a-button type="primary" @click="modalVisible = true">{{ t('versions.install') }}</a-button>
      </div>

      <a-table :columns="columns" :data="store.versions" :pagination="false" row-key="id">
        <template #usedBy="{ record }">
          {{ t('versions.referenced', { count: usedByCount(record.id) }) }}
        </template>
        <template #actions="{ record }">
          <a-popconfirm
            :content="t('versions.confirmDelete', { version: record.version })"
            @ok="onRemove(record.id, record.version)"
          >
            <a-button size="small" status="danger">{{ t('versions.table.delete') }}</a-button>
          </a-popconfirm>
        </template>
        <template #empty>
          <a-empty :description="t('versions.empty')" />
        </template>
      </a-table>
    </div>

    <a-modal
      v-model:visible="modalVisible"
      :title="t('versions.install')"
      :ok-text="t('versions.installNow')"
      :cancel-text="t('common.cancel')"
      :ok-button-props="{ disabled: !pickedVersion || installing }"
      :mask-closable="!installing"
      @ok="onInstall"
    >
      <div class="install-body">
        <a-space>
          <a-button :loading="fetching" @click="onFetch">
            {{ fetching ? t('versions.fetching') : t('versions.fetch') }}
          </a-button>
          <a-select
            v-model="pickedVersion"
            :placeholder="t('versions.selectVersion')"
            style="width: 220px"
            :disabled="installing"
          >
            <a-option v-for="v in installable" :key="v" :value="v">{{ v }}</a-option>
          </a-select>
        </a-space>
        <a-progress v-if="installing" :percent="progress / 100" class="install-progress" />
      </div>
    </a-modal>
  </div>
</template>

<style lang="scss" scoped>
.install-body {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.install-progress {
  width: 100%;
}
</style>
