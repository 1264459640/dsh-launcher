<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Message } from '@arco-design/web-vue'
import { api } from '@/api'
import { useLauncherStore } from '@/stores/launcher'
import type { DshInstance, InstalledPlugin } from '@/api/types'
import TerminalEmbed from './TerminalEmbed.vue'

const route = useRoute()
const router = useRouter()
const { t } = useI18n()
const store = useLauncherStore()

const editingId = computed(() => (route.params.id as string | undefined) ?? null)
const isNew = computed(() => !editingId.value)

// --- Sidebar tabs ---------------------------------------------------------------

type TabKey = 'basic' | 'env' | 'profiles' | 'plugins' | 'terminal'
const activeTab = ref<TabKey>('basic')

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
const addingProfile = ref(false)
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
  await loadIcon()
})

// --- Instance icon (issue #8) --------------------------------------------------

const iconUrl = ref<string | null>(null)
const iconInput = ref('')
const iconBusy = ref(false)

async function loadIcon() {
  if (!editingId.value) return
  try {
    iconUrl.value = await api.readInstanceIcon(editingId.value)
  } catch {
    iconUrl.value = null
  }
}

async function applyIconInput() {
  if (!editingId.value || !iconInput.value.trim()) return
  iconBusy.value = true
  try {
    await api.setInstanceIcon(editingId.value, iconInput.value.trim())
    iconInput.value = ''
    await loadIcon()
    Message.success(t('instanceEdit.iconUpdated'))
  } catch (e) {
    Message.error(String(e))
  } finally {
    iconBusy.value = false
  }
}

async function pickIconFile() {
  if (!editingId.value) return
  const { open } = await import('@tauri-apps/plugin-dialog')
  const file = await open({
    multiple: false,
    filters: [{ name: 'Image', extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif', 'bmp'] }],
  })
  if (typeof file !== 'string') return
  iconBusy.value = true
  try {
    await api.setInstanceIcon(editingId.value, file)
    await loadIcon()
    Message.success(t('instanceEdit.iconUpdated'))
  } catch (e) {
    Message.error(String(e))
  } finally {
    iconBusy.value = false
  }
}

async function clearIcon() {
  if (!editingId.value) return
  try {
    await api.clearInstanceIcon(editingId.value)
    await loadIcon()
  } catch (e) {
    Message.error(String(e))
  }
}

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
    addingProfile.value = false
    Message.success(t('instanceEdit.profileCreated', { name }))
  } catch (e) {
    Message.error(String(e))
  } finally {
    creatingProfile.value = false
  }
}

function cancelAddProfile() {
  addingProfile.value = false
  newProfileName.value = ''
}

function setDefaultProfile(name: string) {
  defaultProfile.value = name
  Message.success(t('instanceEdit.profileSetDefault', { name }))
}

// --- Profile rename/delete ------------------------------------------------------

const renamingProfile = ref<string | null>(null)
const renameValue = ref('')
const busyProfile = ref<string | null>(null)

// --- Profile copy ---------------------------------------------------------------

const copyingProfile = ref<string | null>(null)
const copyProfileName = ref('')
const copyProfileBusy = ref(false)

function startCopyProfile(name: string) {
  copyingProfile.value = name
  copyProfileName.value = `${name}-copy`
}

function cancelCopyProfile() {
  copyingProfile.value = null
  copyProfileName.value = ''
}

async function confirmCopyProfile() {
  if (!homeId.value || !copyingProfile.value) return
  const source = copyingProfile.value
  const newName = copyProfileName.value.trim()
  if (!newName) return
  copyProfileBusy.value = true
  try {
    await api.copyProfile(homeId.value, source, newName)
    profiles.value = await api.listProfiles(homeId.value)
    cancelCopyProfile()
    Message.success(t('instanceEdit.profileCopied', { source, name: newName }))
  } catch (e) {
    Message.error(String(e))
  } finally {
    copyProfileBusy.value = false
  }
}

function startRenameProfile(name: string) {
  renamingProfile.value = name
  renameValue.value = name
}

function cancelRenameProfile() {
  renamingProfile.value = null
  renameValue.value = ''
}

async function confirmRenameProfile() {
  if (!homeId.value || !renamingProfile.value) return
  const oldName = renamingProfile.value
  const newName = renameValue.value.trim()
  if (!newName || newName === oldName) {
    cancelRenameProfile()
    return
  }
  busyProfile.value = oldName
  try {
    await api.renameProfile(homeId.value, oldName, newName)
    profiles.value = await api.listProfiles(homeId.value)
    cancelRenameProfile()
    Message.success(t('instanceEdit.profileRenamed', { old: oldName, name: newName }))
  } catch (e) {
    Message.error(String(e))
  } finally {
    busyProfile.value = null
  }
}

async function confirmDeleteProfile(name: string) {
  if (!homeId.value) return
  busyProfile.value = name
  try {
    await api.deleteProfile(homeId.value, name)
    profiles.value = await api.listProfiles(homeId.value)
    if (defaultProfile.value === name) defaultProfile.value = undefined
    Message.success(t('instanceEdit.profileDeleted', { name }))
  } catch (e) {
    Message.error(String(e))
  } finally {
    busyProfile.value = null
  }
}

// --- Modpack (整合包) export/import ----------------------------------------------

const exportProfile = ref<string | null>(null)
const exportBusy = ref(false)
const exportForm = ref({
  name: '',
  version: '1.0.0',
  displayName: '',
  description: '',
  author: '',
  outDir: '',
})

function startExportModpack(profile: string) {
  exportProfile.value = profile
  exportForm.value = {
    name: profile,
    version: '1.0.0',
    displayName: '',
    description: '',
    author: '',
    outDir: '',
  }
}

async function pickExportDir() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const dir = await open({ directory: true, multiple: false })
  if (typeof dir === 'string') exportForm.value.outDir = dir
}

async function confirmExportModpack() {
  if (!homeId.value || !exportProfile.value || !exportForm.value.outDir) return
  exportBusy.value = true
  try {
    const path = await api.exportModpack({
      home_id: homeId.value,
      profile: exportProfile.value,
      out_dir: exportForm.value.outDir,
      name: exportForm.value.name.trim() || undefined,
      version: exportForm.value.version.trim() || undefined,
      displayName: exportForm.value.displayName.trim() || undefined,
      description: exportForm.value.description.trim() || undefined,
      author: exportForm.value.author.trim() || undefined,
    })
    exportProfile.value = null
    Message.success(t('instanceEdit.modpackExported', { path }))
  } catch (e) {
    Message.error(String(e))
  } finally {
    exportBusy.value = false
  }
}

// --- Launch shortcut (issue #9) -----------------------------------------------

/** Writes a dsh-launcher://launch .url shortcut for this instance + profile. */
async function createShortcut(profile: string) {
  if (!editingId.value) return
  const { save } = await import('@tauri-apps/plugin-dialog')
  const dest = await save({
    defaultPath: `${name.value.trim() || 'instance'}-${profile}.url`,
    filters: [{ name: 'Shortcut', extensions: ['url'] }],
  })
  if (typeof dest !== 'string') return
  try {
    await api.createLaunchShortcut(editingId.value, profile, dest)
    Message.success(t('instanceEdit.shortcutCreated', { path: dest }))
  } catch (e) {
    Message.error(String(e))
  }
}

// --- Plugins tab ---------------------------------------------------------------

const pluginProfile = ref<string>('')
const installedPlugins = ref<InstalledPlugin[]>([])
const pluginsLoading = ref(false)
const selectedPlugins = ref<string[]>([])
const pluginsBusy = ref(false)

const visiblePlugins = computed(() =>
  // Backend already excludes @deepseek-ai/*; double-filter for safety.
  installedPlugins.value.filter((p) => !p.id.startsWith('@deepseek-ai/')),
)

/**
 * 版本号显示：Git commit 哈希（40 位十六进制）只显示前 7 位。
 */
function displayVersion(v: string | undefined): string {
  if (v && /^[0-9a-f]{40}$/i.test(v)) return v.slice(0, 7)
  return v ?? ''
}

watch([pluginProfile, homeId], async () => {
  await loadPlugins()
})

// 进入插件页时若未选择 Profile：优先选中实例的默认 Profile；若实例没有
// 设置默认 Profile，则选中找到的第一个 Profile。
watch(activeTab, async (tab) => {
  if (tab !== 'plugins') return
  if (pluginProfile.value) return
  if (profiles.value.length === 0) return
  if (defaultProfile.value && profiles.value.includes(defaultProfile.value)) {
    pluginProfile.value = defaultProfile.value
  } else {
    pluginProfile.value = profiles.value[0]
  }
})

async function loadPlugins() {
  installedPlugins.value = []
  selectedPlugins.value = []
  if (!editingId.value || !pluginProfile.value) return
  pluginsLoading.value = true
  try {
    installedPlugins.value = await api.listInstalledPlugins(editingId.value, pluginProfile.value)
  } catch (e) {
    Message.error(String(e))
  } finally {
    pluginsLoading.value = false
  }
}

async function onTogglePlugin(p: InstalledPlugin, enabled: boolean) {
  if (!editingId.value || !pluginProfile.value) return
  pluginsBusy.value = true
  try {
    await api.setPluginsEnabled({
      instanceId: editingId.value,
      profile: pluginProfile.value,
      pluginIds: [p.id],
      enabled,
    })
    p.enabled = enabled
    Message.success(
      enabled
        ? t('instanceEdit.pluginEnabled', { name: p.id })
        : t('instanceEdit.pluginDisabled', { name: p.id }),
    )
  } catch (e) {
    Message.error(String(e))
    await loadPlugins()
  } finally {
    pluginsBusy.value = false
  }
}

async function onUninstallPlugin(p: InstalledPlugin) {
  if (!editingId.value || !pluginProfile.value) return
  pluginsBusy.value = true
  try {
    await api.uninstallPlugin({
      instanceId: editingId.value,
      profile: pluginProfile.value,
      pluginId: p.id,
    })
    Message.success(t('instanceEdit.pluginUninstalled', { name: p.id }))
    Message.info(t('instanceEdit.pluginRestartHint'))
    await loadPlugins()
  } catch (e) {
    Message.error(String(e))
  } finally {
    pluginsBusy.value = false
  }
}

function onSwitchChange(p: InstalledPlugin, val: string | number | boolean) {
  onTogglePlugin(p, val === true)
}

async function batchSetEnabled(enabled: boolean) {
  if (!editingId.value || !pluginProfile.value || selectedPlugins.value.length === 0) return
  pluginsBusy.value = true
  const ids = [...selectedPlugins.value]
  try {
    await api.setPluginsEnabled({
      instanceId: editingId.value,
      profile: pluginProfile.value,
      pluginIds: ids,
      enabled,
    })
    for (const p of installedPlugins.value) {
      if (ids.includes(p.id)) p.enabled = enabled
    }
    selectedPlugins.value = []
    Message.success(
      enabled
        ? t('instanceEdit.pluginsBatchEnabled', { count: ids.length })
        : t('instanceEdit.pluginsBatchDisabled', { count: ids.length }),
    )
  } catch (e) {
    Message.error(String(e))
    await loadPlugins()
  } finally {
    pluginsBusy.value = false
  }
}

function onSelectionChange(rowKeys: (string | number)[]) {
  selectedPlugins.value = rowKeys.map(String)
}

const rowSelection = {
  type: 'checkbox' as const,
  showCheckedAll: true,
  onlyCurrent: true,
}

// --- Terminal tab ------------------------------------------------------------

const terminalRunning = ref(false)
</script>

<template>
  <div class="edit-page">
    <aside class="edit-sidebar">
      <a-menu :selected-keys="[activeTab]" @menu-item-click="(key: string) => (activeTab = key as TabKey)">
        <a-menu-item key="basic">{{ t('instanceEdit.tabs.basic') }}</a-menu-item>
        <a-menu-item key="env">{{ t('instanceEdit.tabs.env') }}</a-menu-item>
        <a-menu-item key="profiles">{{ t('instanceEdit.tabs.profiles') }}</a-menu-item>
        <a-menu-item key="plugins">{{ t('instanceEdit.tabs.plugins') }}</a-menu-item>
        <a-menu-item key="terminal">{{ t('instanceEdit.tabs.terminal') }}</a-menu-item>
      </a-menu>
    </aside>
    <section class="edit-content">
      <a-scrollbar type="track" outer-style="height: 100%" style="height: 100%; overflow-y: auto">
        <div class="edit-inner">
          <!-- Basic settings -->
          <div v-if="activeTab === 'basic'" class="dl-card edit-card">
            <a-form layout="vertical" class="edit-form" :model="{}">
              <a-form-item :label="t('instanceEdit.name')" required>
                <a-input v-model="name" :placeholder="t('instanceEdit.namePlaceholder')" style="max-width: 360px" />
              </a-form-item>

              <a-form-item v-if="editingId" :label="t('instanceEdit.icon')">
                <div class="icon-editor">
                  <img v-if="iconUrl" :src="iconUrl" class="icon-preview" alt="" />
                  <div v-else class="icon-preview icon-default">DSH</div>
                  <div class="icon-actions">
                    <a-input
                      v-model="iconInput"
                      :placeholder="t('instanceEdit.iconUrlHint')"
                      allow-clear
                      style="max-width: 300px"
                    />
                    <a-space>
                      <a-button size="small" :loading="iconBusy" :disabled="!iconInput.trim()" @click="applyIconInput">
                        {{ t('instanceEdit.iconApply') }}
                      </a-button>
                      <a-button size="small" :loading="iconBusy" @click="pickIconFile">
                        {{ t('instanceEdit.iconPickFile') }}
                      </a-button>
                      <a-button v-if="iconUrl" size="small" status="danger" @click="clearIcon">
                        {{ t('instanceEdit.iconClear') }}
                      </a-button>
                    </a-space>
                    <p class="icon-hint">{{ t('instanceEdit.iconHint') }}</p>
                  </div>
                </div>
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
            </a-form>

            <div class="footer-actions">
              <a-button type="primary" size="large" :disabled="!formValid" :loading="saving" @click="onSave">
                {{ t('instanceEdit.save') }}
              </a-button>
              <a-button size="large" @click="router.push({ name: 'home' })">{{ t('instanceEdit.cancel') }}</a-button>
            </div>
          </div>

          <!-- Environment overrides -->
          <div v-else-if="activeTab === 'env'" class="dl-card edit-card">
            <h4 class="env-title">{{ t('instanceEdit.env') }}</h4>
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
            <a-button size="small" class="env-add-btn" @click="addEnvRow">{{ t('instanceEdit.envAdd') }}</a-button>

            <div class="footer-actions">
              <a-button type="primary" size="large" :disabled="!formValid" :loading="saving" @click="onSave">
                {{ t('instanceEdit.save') }}
              </a-button>
              <a-button size="large" @click="router.push({ name: 'home' })">{{ t('instanceEdit.cancel') }}</a-button>
            </div>
          </div>

          <!-- Profiles -->
          <div v-else-if="activeTab === 'profiles'" class="dl-card edit-card">
            <h4 class="env-title">{{ t('instanceEdit.tabs.profiles') }}</h4>
            <p class="env-desc">{{ t('instanceEdit.profilesDesc') }}</p>

            <template v-if="homeId && homeId !== DEDICATED">
              <div v-if="profiles.length === 0" class="profiles-empty">
                <a-empty :description="t('instanceEdit.profilesEmpty')" />
              </div>

              <div v-for="p in profiles" :key="p" class="profile-item">
                <template v-if="renamingProfile === p">
                  <a-input
                    v-model="renameValue"
                    class="profile-item-name"
                    :status="renameValue.trim() && renameValue.trim() !== p ? undefined : 'error'"
                    @press-enter="confirmRenameProfile"
                  />
                  <a-button size="small" type="primary" :loading="busyProfile === p" @click="confirmRenameProfile">
                    {{ t('instanceEdit.profileRenameSave') }}
                  </a-button>
                  <a-button size="small" @click="cancelRenameProfile">{{ t('instanceEdit.cancel') }}</a-button>
                </template>
                <template v-else-if="copyingProfile === p">
                  <a-input
                    v-model="copyProfileName"
                    class="profile-item-name"
                    :status="copyProfileName.trim() ? undefined : 'error'"
                    @press-enter="confirmCopyProfile"
                  />
                  <a-button size="small" type="primary" :loading="copyProfileBusy" @click="confirmCopyProfile">
                    {{ t('instanceEdit.profileCopySave') }}
                  </a-button>
                  <a-button size="small" @click="cancelCopyProfile">{{ t('instanceEdit.cancel') }}</a-button>
                </template>
                <template v-else>
                  <span class="profile-item-name">
                    {{ p }}
                    <a-tag v-if="defaultProfile === p" color="arcoblue" size="small">
                      {{ t('instanceEdit.profileDefaultTag') }}
                    </a-tag>
                  </span>
                  <span class="profile-item-actions">
                    <a-button size="small" @click="startRenameProfile(p)">{{ t('instanceEdit.profileRename') }}</a-button>
                    <a-button size="small" @click="startCopyProfile(p)">{{ t('instanceEdit.profileCopy') }}</a-button>
                    <a-button size="small" @click="startExportModpack(p)">{{ t('instanceEdit.modpackExport') }}</a-button>
                    <a-button size="small" @click="createShortcut(p)">{{ t('instanceEdit.createShortcut') }}</a-button>
                    <a-button
                      v-if="defaultProfile !== p"
                      size="small"
                      type="primary"
                      @click="setDefaultProfile(p)"
                    >
                      {{ t('instanceEdit.profileSetDefaultBtn') }}
                    </a-button>
                    <a-popconfirm
                      :content="t('instanceEdit.profileDeleteConfirm', { name: p })"
                      @ok="confirmDeleteProfile(p)"
                    >
                      <a-button size="small" status="danger" :loading="busyProfile === p">
                        {{ t('instances.table.delete') }}
                      </a-button>
                    </a-popconfirm>
                  </span>
                </template>
              </div>

              <div v-if="addingProfile" class="profile-item">
                <a-input
                  v-model="newProfileName"
                  :placeholder="t('instanceEdit.profileCreatePlaceholder')"
                  class="profile-item-name"
                  @press-enter="onCreateProfile"
                />
                <a-button size="small" type="primary" :loading="creatingProfile" @click="onCreateProfile">
                  {{ t('instanceEdit.profileCreate') }}
                </a-button>
                <a-button size="small" @click="cancelAddProfile">{{ t('instanceEdit.cancel') }}</a-button>
              </div>

              <a-button v-if="!addingProfile" size="small" class="profile-add-btn" @click="addingProfile = true">
                {{ t('instanceEdit.profileAdd') }}
              </a-button>
            </template>

            <a-alert v-else type="info">
              {{ t('instanceEdit.profilesNeedHome') }}
            </a-alert>

            <div class="footer-actions">
              <a-button type="primary" size="large" :disabled="!formValid" :loading="saving" @click="onSave">
                {{ t('instanceEdit.save') }}
              </a-button>
              <a-button size="large" @click="router.push({ name: 'home' })">{{ t('instanceEdit.cancel') }}</a-button>
            </div>
          </div>

          <!-- Plugins -->
          <div v-else-if="activeTab === 'plugins'" class="dl-card edit-card">
            <h4 class="env-title">{{ t('instanceEdit.tabs.plugins') }}</h4>
            <p class="env-desc">{{ t('instanceEdit.pluginsDesc') }}</p>

            <template v-if="homeId && homeId !== DEDICATED">
              <div class="plugins-toolbar">
                <a-select
                  v-model="pluginProfile"
                  :placeholder="t('plugins.chooseProfile')"
                  style="width: 220px"
                >
                  <a-option v-for="p in profiles" :key="p" :value="p">{{ p }}</a-option>
                </a-select>
                <a-button
                  size="small"
                  type="text"
                  :disabled="!pluginProfile"
                  :loading="pluginsLoading"
                  @click="loadPlugins"
                >
                  ⟳
                </a-button>
              </div>

              <template v-if="pluginProfile">
                <a-table
                  :data="visiblePlugins"
                  :loading="pluginsLoading"
                  :row-selection="rowSelection"
                  row-key="id"
                  :pagination="false"
                  class="plugins-table"
                  @selection-change="onSelectionChange"
                >
                  <template #columns>
                    <a-table-column title="ID" data-index="id" :width="320">
                      <template #cell="{ record }">
                        <span class="plugin-cell-id">{{ record.id }}</span>
                      </template>
                    </a-table-column>
                    <a-table-column :title="t('instanceEdit.pluginVersion')" data-index="version" :width="140">
                      <template #cell="{ record }">
                        <span v-if="record.version">{{ displayVersion(record.version) }}</span>
                        <span v-else class="plugin-no-version">-</span>
                      </template>
                    </a-table-column>
                    <a-table-column :title="t('instanceEdit.pluginStatus')" data-index="enabled" :width="120">
                      <template #cell="{ record }">
                        <a-switch
                          :model-value="record.enabled"
                          :disabled="pluginsBusy"
                          :checked-text="t('instanceEdit.pluginOn')"
                          :unchecked-text="t('instanceEdit.pluginOff')"
                          @change="onSwitchChange(record, $event)"
                        />
                      </template>
                    </a-table-column>
                    <a-table-column :title="t('instanceEdit.pluginActions')" :width="90">
                      <template #cell="{ record }">
                        <a-popconfirm
                          :content="t('instanceEdit.pluginUninstallConfirm', { name: record.id })"
                          @ok="onUninstallPlugin(record)"
                        >
                          <a-button size="small" status="danger" :disabled="pluginsBusy">
                            {{ t('instances.table.delete') }}
                          </a-button>
                        </a-popconfirm>
                      </template>
                    </a-table-column>
                  </template>
                </a-table>

                <div class="plugins-batch">
                  <a-button
                    size="small"
                    type="primary"
                    :disabled="selectedPlugins.length === 0 || pluginsBusy"
                    @click="batchSetEnabled(true)"
                  >
                    {{ t('instanceEdit.pluginsBatchEnable', { count: selectedPlugins.length }) }}
                  </a-button>
                  <a-button
                    size="small"
                    status="danger"
                    :disabled="selectedPlugins.length === 0 || pluginsBusy"
                    @click="batchSetEnabled(false)"
                  >
                    {{ t('instanceEdit.pluginsBatchDisable', { count: selectedPlugins.length }) }}
                  </a-button>
                </div>

                <a-empty
                  v-if="!pluginsLoading && visiblePlugins.length === 0"
                  :description="t('instanceEdit.pluginsEmpty')"
                />
              </template>
              <a-empty v-else :description="t('instanceEdit.pluginsPickProfile')" />
            </template>

            <a-alert v-else type="info">
              {{ t('instanceEdit.profilesNeedHome') }}
            </a-alert>
          </div>

          <!-- Terminal -->
          <div v-else class="dl-card edit-card">
            <h4 class="env-title">{{ t('instanceEdit.tabs.terminal') }}</h4>
            <p class="env-desc">{{ t('instanceEdit.terminalDesc') }}</p>

            <template v-if="editingId">
              <TerminalEmbed
                v-if="editingId"
                :key="editingId"
                :instance-id="editingId"
                class="terminal-embed"
                @status="(v: boolean) => (terminalRunning = v)"
              />
            </template>

            <a-alert v-else type="info">
              {{ t('instanceEdit.terminalNoHome') }}
            </a-alert>
          </div>
        </div>
      </a-scrollbar>
    </section>

    <!-- Modpack export -->
    <a-modal
      :visible="exportProfile !== null"
      :title="t('instanceEdit.modpackExportTitle', { name: exportProfile ?? '' })"
      :ok-loading="exportBusy"
      :ok-button-props="{ disabled: !exportForm.outDir }"
      @ok="confirmExportModpack"
      @cancel="exportProfile = null"
    >
      <a-form :model="exportForm" layout="vertical">
        <a-form-item :label="t('instanceEdit.modpackName')">
          <a-input v-model="exportForm.name" />
        </a-form-item>
        <a-form-item :label="t('instanceEdit.modpackVersion')">
          <a-input v-model="exportForm.version" placeholder="1.0.0" />
        </a-form-item>
        <a-form-item :label="t('instanceEdit.modpackDisplayName')">
          <a-input v-model="exportForm.displayName" />
        </a-form-item>
        <a-form-item :label="t('instanceEdit.modpackDescription')">
          <a-textarea v-model="exportForm.description" :auto-size="{ minRows: 2, maxRows: 4 }" />
        </a-form-item>
        <a-form-item :label="t('instanceEdit.modpackAuthor')">
          <a-input v-model="exportForm.author" />
        </a-form-item>
        <a-form-item :label="t('instanceEdit.modpackOutDir')" required>
          <a-input v-model="exportForm.outDir" readonly :placeholder="t('instanceEdit.modpackOutDirHint')">
            <template #append>
              <a-button @click="pickExportDir">{{ t('settings.pickDir') }}</a-button>
            </template>
          </a-input>
        </a-form-item>
      </a-form>
    </a-modal>

  </div>
</template>
<style lang="scss" scoped>
.icon-editor {
  display: flex;
  gap: 16px;
  align-items: flex-start;
}

.icon-preview {
  width: 64px;
  height: 64px;
  border-radius: 12px;
  object-fit: cover;
  flex-shrink: 0;
  border: 1px solid var(--color-border-2);
}

.icon-default {
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 700;
  color: #fff;
  background: linear-gradient(135deg, #4d6bfe, #165dff);
}

.icon-actions {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.icon-hint {
  margin: 0;
  font-size: 12px;
  color: var(--color-text-3);
}

.edit-page {
  display: flex;
  height: calc(100vh - var(--dl-header-height));
}

.edit-sidebar {
  width: 200px;
  flex-shrink: 0;
  background: var(--color-bg-2);
  border-right: 1px solid var(--color-border-2);

  :deep(.arco-menu) {
    height: 100%;
  }
}

.edit-content {
  flex: 1;
  min-width: 0;
  overflow: hidden;
}

.edit-inner {
  padding: 20px 24px 80px;
}

.edit-card {
  // Full-width card: stretch to fill the content area like the download page.
  width: 100%;
  box-sizing: border-box;
}

.edit-form {
  width: 100%;
}

.dedicated-hint {
  margin-top: 8px;
  max-width: 360px;
}

.profile-item {
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 10px 12px;
  border: 1px solid var(--color-border-2);
  border-radius: 6px;
  margin-bottom: 8px;
  background: var(--color-fill-1);
}

.profile-item-name {
  flex: 1;
  min-width: 0;
  font-size: 14px;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.profile-item-actions {
  display: inline-flex;
  gap: 8px;
  align-items: center;
  flex-shrink: 0;
}

.profiles-empty {
  padding: 8px 0;
}

.profile-add-btn {
  margin-top: 4px;
}

.env-title {
  margin: 0 0 4px;
  font-size: 15px;
}

.env-desc {
  margin-top: 0;
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

.env-add-btn {
  margin-top: 4px;
}

.plugins-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
}

.plugins-table {
  margin-bottom: 12px;
}

.plugin-cell-id {
  font-family: monospace;
  font-size: 13px;
}

.plugin-no-version {
  color: var(--color-text-4);
}

.plugins-batch {
  display: flex;
  gap: 8px;
  margin-bottom: 8px;
}

.footer-actions {
  margin-top: 20px;
  display: flex;
  gap: 12px;
  justify-content: center;
}

.terminal-row {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-bottom: 8px;
}

.terminal-embed {
  height: 480px;
}

.terminal-hint {
  margin: 4px 0 12px;
  color: var(--color-text-3);
  font-size: 12px;
}

.terminal-alert {
  margin-top: 8px;
  max-width: 640px;
}

@media (max-width: 720px) {
  .edit-page {
    flex-direction: column;
  }

  .edit-sidebar {
    width: 100%;
    height: auto;
    border-right: none;
    border-bottom: 1px solid var(--color-border-2);

    :deep(.arco-menu) {
      height: auto;
      display: flex;
      overflow-x: auto;
    }

    :deep(.arco-menu-item) {
      white-space: nowrap;
    }
  }
}
</style>
