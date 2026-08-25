<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Message } from '@arco-design/web-vue'
import { api } from '@/api'
import { useLauncherStore } from '@/stores/launcher'

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

const selectedVersion = computed(() =>
  selectedInstance.value ? store.versionById(selectedInstance.value.version_id) : undefined,
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

const launchSubtitle = computed(() => {
  if (!selectedInstance.value) return ''
  const v = selectedVersion.value?.version ?? '?'
  const p = selectedProfile.value ?? '—'
  return `${v} · ${p}`
})

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

function copyUrl(url: string) {
  navigator.clipboard?.writeText(url)
  Message.success(t('common.copied'))
}

function goEditSelected() {
  if (selectedInstanceId.value) {
    router.push({ name: 'instance-edit', params: { id: selectedInstanceId.value } })
  }
}
</script>

<template>
  <div class="home-page">
    <!-- Left launch panel -->
    <aside class="launch-panel">
      <div class="identity-block">
        <div class="instance-avatar">⚡</div>
        <div class="instance-name">{{ selectedInstance?.name ?? '—' }}</div>
        <a-tag
          v-if="selectedStatus"
          :color="selectedStatus.state === 'running' ? 'green' : selectedStatus.state === 'starting' ? 'orange' : 'gray'"
          size="small"
        >
          {{ t(`home.status.${selectedStatus.state}`) }}
        </a-tag>
        <div v-if="running && selectedStatus?.url" class="running-url">
          <a-link :href="selectedStatus.url" target="_blank">{{ selectedStatus.url }}</a-link>
          <a-button size="mini" type="text" @click="copyUrl(selectedStatus.url)">
            {{ t('common.copy') }}
          </a-button>
        </div>
        <a-tooltip v-if="sharedHome" :content="t('home.sharedHomeWarning')">
          <a-tag color="orangered" size="small">{{ t('home.sharedHome') }}</a-tag>
        </a-tooltip>
      </div>

      <div class="selector-block">
        <div class="field">
          <span class="field-label">{{ t('home.instance') }}</span>
          <a-select
            v-model="selectedInstanceId"
            :placeholder="t('home.selectInstance')"
            allow-clear
          >
            <a-option v-for="inst in store.instances" :key="inst.id" :value="inst.id">
              <span class="option-line">
                {{ inst.name }}
                <a-tag
                  v-if="store.statusOf(inst.id).state === 'running'"
                  size="small"
                  color="green"
                >
                  {{ t('home.status.running') }}
                </a-tag>
              </span>
            </a-option>
          </a-select>
        </div>
        <div class="field">
          <span class="field-label">{{ t('home.profile') }}</span>
          <a-select
            v-model="selectedProfile"
            :placeholder="t('home.selectProfile')"
            :loading="profilesLoading"
            :disabled="!selectedInstance"
            allow-clear
          >
            <a-option v-for="p in profiles" :key="p" :value="p">{{ p }}</a-option>
          </a-select>
        </div>
      </div>

      <div class="action-block">
        <template v-if="!running">
          <a-button
            type="primary"
            size="large"
            long
            :disabled="!canStart"
            :loading="starting"
            class="launch-button"
            @click="onStart"
          >
            <span class="launch-text">{{ starting ? t('home.starting') : t('home.start') }}</span>
            <span v-if="launchSubtitle && !starting" class="launch-sub">{{ launchSubtitle }}</span>
          </a-button>
        </template>
        <template v-else>
          <a-button type="primary" size="large" long class="launch-button" @click="onOpenWindow">
            <span class="launch-text">{{ t('home.openWindow') }}</span>
            <span class="launch-sub">{{ launchSubtitle }}</span>
          </a-button>
          <a-button status="danger" long class="stop-button" @click="onStop">
            {{ t('home.stop') }}
          </a-button>
        </template>
        <div class="mini-actions">
          <a-button class="mini-button" @click="router.push({ name: 'instances' })">
            {{ t('home.instanceList') }}
          </a-button>
          <a-button class="mini-button" :disabled="!selectedInstance" @click="goEditSelected">
            {{ t('home.editSelected') }}
          </a-button>
        </div>
      </div>
    </aside>

    <!-- Right news area (reserved) -->
    <section class="news-area">
      <div class="news-placeholder">{{ t('home.newsPlaceholder') }}</div>
    </section>
  </div>
</template>

<style lang="scss" scoped>
.home-page {
  display: flex;
  height: calc(100vh - var(--dl-header-height));
}

.launch-panel {
  width: 320px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  padding: 20px 16px;
  background: var(--color-bg-2);
  border-right: 1px solid var(--color-border-2);
}

.selector-block {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin-bottom: 14px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.field-label {
  font-size: 12px;
  color: var(--color-text-3);
}

.identity-block {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  min-height: 0;
}

.instance-avatar {
  width: 88px;
  height: 88px;
  border-radius: 16px;
  background: linear-gradient(135deg, #165dff, #722ed1);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 40px;
  box-shadow: 0 6px 16px rgb(22 93 255 / 25%);
  user-select: none;
}

.instance-name {
  font-size: 18px;
  font-weight: 600;
}

.running-url {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
}

.action-block {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.launch-button {
  height: 64px;
  display: flex;
  flex-direction: column;

  .launch-text {
    font-size: 17px;
    font-weight: 600;
  }

  .launch-sub {
    font-size: 12px;
    opacity: 0.8;
    margin-top: 2px;
  }
}

.stop-button {
  height: 40px;
}

.mini-actions {
  display: flex;
  gap: 10px;
  justify-content: center;
}

.mini-button {
  max-width: 132px;
  width: 50%;
}

.news-area {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  background:
    radial-gradient(circle at 30% 20%, rgb(22 93 255 / 6%), transparent 40%),
    radial-gradient(circle at 70% 80%, rgb(114 46 209 / 6%), transparent 40%);
}

.news-placeholder {
  color: var(--color-text-4);
  font-size: 14px;
  letter-spacing: 4px;
  user-select: none;
}

.option-line {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
</style>
