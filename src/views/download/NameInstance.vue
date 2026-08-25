<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Message } from '@arco-design/web-vue'
import { api } from '@/api'
import { useLauncherStore } from '@/stores/launcher'
import type { InstallProgress } from '@/api/types'

const route = useRoute()
const router = useRouter()
const { t } = useI18n()
const store = useLauncherStore()

const version = computed(() => String(route.params.version ?? ''))
const installedVersion = computed(() => store.versions.find((v) => v.version === version.value))

// Default instance name: version string, deduplicated against existing names.
function suggestName(): string {
  let candidate = version.value
  let n = 2
  while (store.instances.some((i) => i.name === candidate)) {
    candidate = `${version.value}-${n}`
    n += 1
  }
  return candidate
}

const instanceName = ref(suggestName())
const homeId = ref<string | undefined>(store.homes[0]?.id ?? undefined)
const busy = ref(false)
const installing = ref(false)
const progress = ref(0)

let unlisten: (() => void) | null = null

onMounted(async () => {
  unlisten = await api.onInstallProgress((p: InstallProgress) => {
    if (p.version !== version.value) return
    progress.value = p.percent
  })
})

onBeforeUnmount(() => unlisten?.())

const canConfirm = computed(
  () =>
    !busy.value &&
    instanceName.value.trim().length > 0 &&
    !!homeId.value &&
    !store.instances.some((i) => i.name === instanceName.value.trim()),
)

async function onConfirm() {
  if (!canConfirm.value) return
  busy.value = true
  try {
    // 1. Install the DSH version if it is not installed yet.
    if (!installedVersion.value) {
      installing.value = true
      progress.value = 0
      await api.installVersion(version.value)
      await store.refreshVersions()
      installing.value = false
    }
    const ver = store.versions.find((v) => v.version === version.value)
    if (!ver) throw new Error(`version ${version.value} not installed`)

    // 2. Create the instance bound to that version.
    const inst = await api.createInstance({
      name: instanceName.value.trim(),
      version_id: ver.id,
      home_id: homeId.value!,
      env_overrides: {},
      default_profile: null,
    })
    await Promise.all([store.refreshInstances(), api.updateSettings({ last_instance_id: inst.id })])
    store.settings.last_instance_id = inst.id
    Message.success(t('download.created', { name: inst.name }))
    router.push({ name: 'home' })
  } catch (e) {
    installing.value = false
    Message.error(String(e))
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="name-page">
    <!-- Header: back + version icon + name input -->
    <div class="dl-card name-header">
      <a-button type="text" class="back-button" @click="router.push({ name: 'download-create' })">
        ←
      </a-button>
      <span class="version-icon">◆</span>
      <a-input
        v-model="instanceName"
        :placeholder="t('download.instanceName')"
        class="name-input"
        size="large"
      />
    </div>

    <!-- DSH_HOME selection -->
    <div class="dl-card">
      <div class="dl-card-title">
        <h3>{{ t('download.chooseHome') }}</h3>
      </div>
      <template v-if="store.homes.length">
        <a-select v-model="homeId" style="width: 100%; max-width: 480px">
          <a-option v-for="h in store.homes" :key="h.id" :value="h.id">
            {{ h.name }}（{{ h.path }}）
          </a-option>
        </a-select>
      </template>
      <a-alert v-else type="warning">
        {{ t('download.noHome') }}
        <a-link @click="router.push({ name: 'settings' })">{{ t('download.goSettings') }}</a-link>
      </a-alert>
    </div>

    <!-- Action -->
    <div class="confirm-area">
      <a-alert v-if="installedVersion" type="info" class="confirm-hint">
        {{ t('download.alreadyInstalled') }}
      </a-alert>
      <a-alert v-else type="info" class="confirm-hint">
        {{ t('download.willInstall', { version }) }}
      </a-alert>
      <a-progress v-if="installing" :percent="progress / 100" class="confirm-progress" />
      <a-button
        type="primary"
        size="large"
        class="confirm-button"
        :disabled="!canConfirm"
        :loading="busy"
        @click="onConfirm"
      >
        {{ installing ? t('download.downloading') : installedVersion ? t('download.createOnly') : t('download.startDownload') }}
      </a-button>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.name-page {
  max-width: 860px;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-height: calc(100vh - var(--dl-header-height) - 120px);
}

.name-header {
  display: flex;
  align-items: center;
  gap: 12px;
}

.back-button {
  font-size: 18px;
  padding: 0 8px;
}

.version-icon {
  width: 40px;
  height: 40px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, #0fc6c2, #165dff);
  color: #fff;
  font-size: 16px;
  flex-shrink: 0;
}

.name-input {
  flex: 1;
}

.confirm-area {
  margin-top: auto;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 14px;
  padding-top: 24px;
}

.confirm-hint {
  max-width: 520px;
}

.confirm-progress {
  max-width: 520px;
  width: 100%;
}

.confirm-button {
  min-width: 220px;
  height: 48px;
  border-radius: 24px;
  font-size: 16px;
  font-weight: 600;
}
</style>
