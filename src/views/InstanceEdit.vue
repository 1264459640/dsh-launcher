<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Message } from '@arco-design/web-vue'
import { api } from '@/api'
import { useLauncherStore } from '@/stores/launcher'
import type { DshInstance } from '@/api/types'

const route = useRoute()
const router = useRouter()
const { t } = useI18n()
const store = useLauncherStore()

const editingId = computed(() => (route.params.id as string | undefined) ?? null)
const isNew = computed(() => !editingId.value)

// --- Form state ---------------------------------------------------------------

const name = ref('')
const versionId = ref<string | undefined>(undefined)
const DEDICATED = '__dedicated__'
const homeId = ref<string | undefined>(undefined)
const dedicatedPath = ref('')
const defaultProfile = ref<string | undefined>(undefined)
const profiles = ref<string[]>([])
const newProfileName = ref('')
const creatingProfile = ref(false)
const saving = ref(false)

interface EnvRow {
  key: string
  value: string
}
const envRows = ref<EnvRow[]>([])

const ENV_KEY_RE = /^[A-Za-z_][A-Za-z0-9_]*$/
const RESERVED_KEYS = new Set(['DSH_HOME'])

function envKeyError(row: EnvRow): string | null {
  if (!row.key) return null
  if (RESERVED_KEYS.has(row.key)) return t('instanceEdit.envKeyReserved')
  if (!ENV_KEY_RE.test(row.key)) return t('instanceEdit.envKeyInvalid')
  return null
}

const envValid = computed(() => envRows.value.every((r) => !envKeyError(r)))

onMounted(async () => {
  if (!editingId.value) return
  const inst = store.instanceById(editingId.value) ?? (await api.listInstances()).find((i) => i.id === editingId.value)
  if (!inst) {
    Message.error(t('instanceEdit.notFound'))
    router.replace({ name: 'home' })
    return
  }
  name.value = inst.name
  versionId.value = inst.version_id
  homeId.value = inst.home_id
  defaultProfile.value = inst.default_profile ?? undefined
  envRows.value = Object.entries(inst.env_overrides).map(([key, value]) => ({ key, value }))
})

watch(homeId, async (v) => {
  profiles.value = []
  if (v === DEDICATED) {
    dedicatedPath.value = await api.defaultDedicatedHomePath(name.value.trim() || 'instance')
    return
  }
  if (!v) return
  try {
    profiles.value = await api.listProfiles(v)
    if (defaultProfile.value && !profiles.value.includes(defaultProfile.value)) {
      defaultProfile.value = undefined
    }
  } catch (e) {
    Message.error(String(e))
  }
})

watch(name, async (v) => {
  if (homeId.value === DEDICATED) {
    dedicatedPath.value = await api.defaultDedicatedHomePath(v.trim() || 'instance')
  }
})

// --- Save ----------------------------------------------------------------------

const formValid = computed(
  () => name.value.trim().length > 0 && !!versionId.value && !!homeId.value && envValid.value,
)

async function onSave() {
  if (!formValid.value) return
  const envOverrides: Record<string, string> = {}
  for (const row of envRows.value) {
    if (row.key) envOverrides[row.key] = row.value
  }
  saving.value = true
  try {
    // A dedicated DSH_HOME is created on demand for this instance.
    let resolvedHomeId = homeId.value!
    if (homeId.value === DEDICATED) {
      const home = await api.createHome(name.value.trim(), dedicatedPath.value)
      resolvedHomeId = home.id
      await store.refreshHomes()
    }
    if (isNew.value) {
      await api.createInstance({
        name: name.value.trim(),
        version_id: versionId.value!,
        home_id: resolvedHomeId,
        env_overrides: envOverrides,
        default_profile: defaultProfile.value ?? null,
      })
    } else {
      const inst = store.instanceById(editingId.value!) as DshInstance
      await api.updateInstance({
        ...inst,
        name: name.value.trim(),
        version_id: versionId.value!,
        home_id: resolvedHomeId,
        env_overrides: envOverrides,
        default_profile: defaultProfile.value ?? null,
      })
    }
    await store.refreshInstances()
    Message.success(t('instanceEdit.saved'))
    router.push({ name: 'home' })
  } catch (e) {
    Message.error(String(e))
  } finally {
    saving.value = false
  }
}

function addEnvRow() {
  envRows.value.push({ key: '', value: '' })
}

function removeEnvRow(idx: number) {
  envRows.value.splice(idx, 1)
}

async function onCreateProfile() {
  const name = newProfileName.value.trim()
  if (!homeId.value || !name) return
  creatingProfile.value = true
  try {
    await api.createProfile(homeId.value, name)
    profiles.value = await api.listProfiles(homeId.value)
    newProfileName.value = ''
    defaultProfile.value = name
    Message.success(t('instanceEdit.profileCreated', { name }))
  } catch (e) {
    Message.error(String(e))
  } finally {
    creatingProfile.value = false
  }
}
</script>

<template>
  <div class="dl-page">
    <div class="dl-card">
      <div class="dl-card-title">
        <h3>{{ isNew ? t('instanceEdit.titleNew') : t('instanceEdit.titleEdit') }}</h3>
      </div>

      <a-form layout="vertical" class="edit-form" :model="{}">
        <a-form-item :label="t('instanceEdit.name')" required>
          <a-input v-model="name" :placeholder="t('instanceEdit.namePlaceholder')" style="max-width: 360px" />
        </a-form-item>

        <a-form-item :label="t('instanceEdit.version')" required>
          <template v-if="store.versions.length">
            <a-select v-model="versionId" style="max-width: 360px">
              <a-option v-for="v in store.versions" :key="v.id" :value="v.id">{{ v.version }}</a-option>
            </a-select>
          </template>
          <a-alert v-else type="warning">
            {{ t('instanceEdit.noVersion') }}
            <a-link @click="router.push({ name: 'download' })">{{ t('instanceEdit.goDownload') }}</a-link>
          </a-alert>
        </a-form-item>

        <a-form-item :label="t('instanceEdit.home')" required>
          <a-select v-model="homeId" style="max-width: 360px">
            <a-option :value="DEDICATED">{{ t('instanceEdit.dedicatedHome') }}</a-option>
            <a-option v-for="h in store.homes" :key="h.id" :value="h.id">
              {{ h.name }}（{{ h.path }}）
            </a-option>
          </a-select>
          <a-alert v-if="homeId === DEDICATED" type="info" class="dedicated-hint">
            {{ t('instanceEdit.dedicatedHomeHint', { path: dedicatedPath }) }}
          </a-alert>
        </a-form-item>

        <a-form-item :label="t('instanceEdit.defaultProfile')">
          <div class="profile-row">
            <a-select
              v-model="defaultProfile"
              :placeholder="t('instanceEdit.defaultProfilePlaceholder')"
              class="profile-select"
              allow-clear
              :disabled="!homeId || homeId === DEDICATED"
            >
              <a-option v-for="p in profiles" :key="p" :value="p">{{ p }}</a-option>
            </a-select>
            <template v-if="homeId && homeId !== DEDICATED">
              <a-input
                v-model="newProfileName"
                :placeholder="t('instanceEdit.profileCreatePlaceholder')"
                class="profile-create-input"
                @press-enter="onCreateProfile"
              />
              <a-button
                type="primary"
                class="profile-create-btn"
                :loading="creatingProfile"
                :disabled="!newProfileName.trim()"
                @click="onCreateProfile"
              >
                {{ t('instanceEdit.profileCreate') }}
              </a-button>
            </template>
          </div>
        </a-form-item>
      </a-form>
    </div>

    <div class="dl-card">
      <div class="dl-card-title">
        <h3>{{ t('instanceEdit.env') }}</h3>
        <a-button size="small" @click="addEnvRow">{{ t('instanceEdit.envAdd') }}</a-button>
      </div>
      <p class="env-desc">{{ t('instanceEdit.envDesc') }}</p>

      <div v-for="(row, idx) in envRows" :key="idx" class="env-row">
        <a-input
          v-model="row.key"
          :placeholder="t('instanceEdit.envKey')"
          :status="envKeyError(row) ? 'error' : undefined"
          class="env-key"
        />
        <a-input v-model="row.value" :placeholder="t('instanceEdit.envValue')" class="env-value" />
        <a-button status="danger" type="text" @click="removeEnvRow(idx)">
          {{ t('instances.table.delete') }}
        </a-button>
        <div v-if="envKeyError(row)" class="env-error">{{ envKeyError(row) }}</div>
      </div>
      <a-empty v-if="envRows.length === 0" :description="t('instanceEdit.envAdd')" />
    </div>

    <div class="footer-actions">
      <a-button type="primary" size="large" :disabled="!formValid" :loading="saving" @click="onSave">
        {{ t('instanceEdit.save') }}
      </a-button>
      <a-button size="large" @click="router.push({ name: 'home' })">{{ t('instanceEdit.cancel') }}</a-button>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.edit-form {
  max-width: 640px;
}

.dedicated-hint {
  margin-top: 8px;
  max-width: 360px;
}

.profile-row {
  display: flex;
  gap: 8px;
  align-items: center;
  width: 100%;

  :deep(.arco-select-view-single) {
    height: 32px;
    box-sizing: border-box;
  }

  :deep(.arco-input-wrapper) {
    height: 32px;
    box-sizing: border-box;
  }
}

.profile-select {
  width: 200px;
  flex-shrink: 0;
}

.profile-create-input {
  flex: 1;
  min-width: 0;
}

.profile-create-btn {
  flex-shrink: 0;
  height: 32px;
}

.env-desc {
  margin-top: -8px;
  color: var(--color-text-3);
  font-size: 13px;
}

.env-row {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
  margin-bottom: 10px;
}

.env-key {
  width: 240px;
  font-family: monospace;
}

.env-value {
  flex: 1;
  min-width: 220px;
}

.env-error {
  width: 100%;
  color: rgb(var(--red-6));
  font-size: 12px;
}

.footer-actions {
  margin-top: 20px;
  display: flex;
  gap: 12px;
  justify-content: center;
}
</style>
