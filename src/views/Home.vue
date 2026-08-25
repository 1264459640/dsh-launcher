<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Message } from '@arco-design/web-vue'
import { api } from '@/api'
import { useLauncherStore } from '@/stores/launcher'
import type { InstanceState } from '@/api/types'

const router = useRouter()
const { t } = useI18n()
const store = useLauncherStore()

// --- Linked dual dropdowns: instance -> profiles of its DSH_HOME ----------

const selectedInstanceId = ref<string | undefined>(store.settings.last_instance_id ?? undefined)
const profiles = ref<string[]>([])
const selectedProfile = ref<string | undefined>(undefined)
const profilesLoading = ref(false)

const selectedInstance = computed(() =>
  selectedInstanceId.value ? store.instanceById(selectedInstanceId.value) : undefined,
)

const selectedStatus = computed(() =>
  selectedInstanceId.value ? store.statusOf(selectedInstanceId.value) : undefined,
)

const sharedHome = computed(() => {
  if (!selectedInstance.value) return false
  return store.instances.filter((i) => i.home_id === selectedInstance.value!.home_id).length > 1
})

async function loadProfiles() {
  profiles.value = []
  selectedProfile.value = undefined
  const inst = selectedInstance.value
  if (!inst) return
  profilesLoading.value = true
  try {
    profiles.value = await api.listProfiles(inst.home_id)
    selectedProfile.value =
      (inst.last_profile && profiles.value.includes(inst.last_profile) && inst.last_profile) ||
      (inst.default_profile && profiles.value.includes(inst.default_profile) && inst.default_profile) ||
      profiles.value[0] ||
      undefined
    if (profiles.value.length === 0) {
      Message.warning(t('home.noProfile'))
    }
  } catch (e) {
    Message.error(t('common.operationFailed', { msg: String(e) }))
  } finally {
    profilesLoading.value = false
  }
}

watch(selectedInstanceId, () => {
  loadProfiles()
  if (selectedInstanceId.value) {
    api.updateSettings({ last_instance_id: selectedInstanceId.value }).then((s) => {
      store.settings = s
    })
  }
})

watch(
  () => store.instances,
  () => {
    // Selected instance was deleted: fall back.
    if (selectedInstanceId.value && !store.instanceById(selectedInstanceId.value)) {
      selectedInstanceId.value = store.instances[0]?.id ?? undefined
    }
    if (!selectedInstanceId.value && store.instances.length > 0) {
      selectedInstanceId.value =
        store.settings.last_instance_id ?? store.instances[0]?.id ?? undefined
    }
  },
  { deep: true, immediate: true },
)

// --- Start / stop / open ---------------------------------------------------

const starting = computed(() => selectedStatus.value?.state === 'starting')
const running = computed(() => selectedStatus.value?.state === 'running')

const canStart = computed(
  () =>
    !!selectedInstance.value &&
    !!selectedProfile.value &&
    !starting.value &&
    !running.value &&
    !!store.versionById(selectedInstance.value.version_id),
)

async function onStart() {
  if (!selectedInstanceId.value || !selectedProfile.value) return
  try {
    await api.startInstance(selectedInstanceId.value, selectedProfile.value)
    Message.success(t('home.started'))
  } catch (e) {
    Message.error(String(e))
  }
}

async function onStop() {
  if (!selectedInstanceId.value) return
  try {
    await api.stopInstance(selectedInstanceId.value)
    Message.success(t('home.stopped'))
  } catch (e) {
    Message.error(String(e))
  }
}

async function onOpenWindow() {
  if (!selectedInstanceId.value) return
  try {
    await api.openInstanceWindow(selectedInstanceId.value)
  } catch (e) {
    Message.error(String(e))
  }
}

// --- Instance list ----------------------------------------------------------

const columns = computed(() => [
  { title: t('home.table.name'), dataIndex: 'name', width: 180 },
  { title: t('home.table.version'), slotName: 'version', width: 140 },
  { title: t('home.table.home'), slotName: 'home', width: 160 },
  { title: t('home.table.status'), slotName: 'status' },
  { title: t('home.table.actions'), slotName: 'actions', width: 150, align: 'center' as const },
])

function stateTag(state: InstanceState): { color: string } {
  switch (state) {
    case 'running':
      return { color: 'green' }
    case 'starting':
      return { color: 'orange' }
    case 'exited':
      return { color: 'red' }
    default:
      return { color: 'gray' }
  }
}

async function onDelete(id: string, name: string) {
  try {
    await api.deleteInstance(id)
    await store.refreshInstances()
    Message.success(t('home.deleted'))
  } catch (e) {
    Message.error(String(e))
  }
}

function copyUrl(url: string) {
  navigator.clipboard?.writeText(url)
  Message.success(t('common.copied'))
}
</script>

<template>
  <div class="dl-page">
    <!-- Launch area: linked instance/profile selectors + one-click start -->
    <div class="dl-card">
      <div class="dl-card-title">
        <h3>{{ t('nav.home') }}</h3>
      </div>
      <div class="launch-row">
        <a-select
          v-model="selectedInstanceId"
          :placeholder="t('home.selectInstance')"
          class="launch-select"
          allow-clear
        >
          <a-option v-for="inst in store.instances" :key="inst.id" :value="inst.id">
            <span class="option-line">
              {{ inst.name }}
              <a-tag
                v-if="store.statusOf(inst.id).state === 'running'"
                size="small"
                color="green"
                class="option-tag"
              >
                {{ t('home.status.running') }}
              </a-tag>
            </span>
          </a-option>
        </a-select>

        <a-select
          v-model="selectedProfile"
          :placeholder="t('home.selectProfile')"
          :loading="profilesLoading"
          :disabled="!selectedInstance"
          class="launch-select"
          allow-clear
        >
          <a-option v-for="p in profiles" :key="p" :value="p">{{ p }}</a-option>
        </a-select>

        <template v-if="!running">
          <a-button
            type="primary"
            size="large"
            :disabled="!canStart"
            :loading="starting"
            @click="onStart"
          >
            {{ starting ? t('home.starting') : t('home.start') }}
          </a-button>
        </template>
        <template v-else>
          <a-button type="primary" size="large" @click="onOpenWindow">
            {{ t('home.openWindow') }}
          </a-button>
          <a-button status="danger" size="large" @click="onStop">{{ t('home.stop') }}</a-button>
        </template>
      </div>

      <div v-if="running && selectedStatus?.url" class="running-url">
        {{ t('home.runningAt') }}
        <a-link :href="selectedStatus.url" target="_blank">{{ selectedStatus.url }}</a-link>
        <a-button size="mini" type="text" @click="copyUrl(selectedStatus.url)">
          {{ t('common.copy') }}
        </a-button>
      </div>

      <a-alert v-if="sharedHome" type="warning" class="shared-hint" :show-icon="true">
        {{ t('home.sharedHomeWarning') }}
      </a-alert>
    </div>

    <!-- Instance list -->
    <div class="dl-card">
      <div class="dl-card-title">
        <h3>{{ t('home.instanceList') }}</h3>
        <div class="dl-toolbar">
          <a-button type="primary" @click="router.push({ name: 'instance-new' })">
            {{ t('home.newInstance') }}
          </a-button>
          <a-button @click="router.push({ name: 'versions' })">{{ t('home.goVersions') }}</a-button>
        </div>
      </div>

      <a-table :columns="columns" :data="store.instances" :pagination="false" row-key="id">
        <template #version="{ record }">
          {{ store.versionById(record.version_id)?.version ?? record.version_id }}
        </template>
        <template #home="{ record }">
          {{ store.homeById(record.home_id)?.name ?? record.home_id }}
        </template>
        <template #status="{ record }">
          <a-tag :color="stateTag(store.statusOf(record.id).state).color">
            {{ t(`home.status.${store.statusOf(record.id).state}`) }}
          </a-tag>
          <a-link
            v-if="store.statusOf(record.id).url"
            :href="store.statusOf(record.id).url!"
            target="_blank"
            class="status-url"
          >
            {{ store.statusOf(record.id).url }}
          </a-link>
        </template>
        <template #actions="{ record }">
          <a-space>
            <a-button size="small" @click="router.push({ name: 'instance-edit', params: { id: record.id } })">
              {{ t('home.table.edit') }}
            </a-button>
            <a-popconfirm
              :content="t('home.confirmDelete', { name: record.name })"
              @ok="onDelete(record.id, record.name)"
            >
              <a-button size="small" status="danger">{{ t('home.table.delete') }}</a-button>
            </a-popconfirm>
          </a-space>
        </template>
        <template #empty>
          <a-empty :description="t('home.emptyDesc')">
            <template #image>
              <div class="empty-title">{{ t('home.emptyTitle') }}</div>
            </template>
            <a-button type="primary" @click="router.push({ name: 'instance-new' })">
              {{ t('home.newInstance') }}
            </a-button>
          </a-empty>
        </template>
      </a-table>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.launch-row {
  display: flex;
  gap: 12px;
  align-items: center;
  flex-wrap: wrap;
}

.launch-select {
  width: 240px;
}

.option-line {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.running-url {
  margin-top: 12px;
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--color-text-2);
}

.shared-hint {
  margin-top: 12px;
}

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
