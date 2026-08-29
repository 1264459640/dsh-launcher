<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Message } from '@arco-design/web-vue'
import { api } from '@/api'
import type { ModpackManifest } from '@/api/types'
import { useLauncherStore } from '@/stores/launcher'

const props = defineProps<{
  visible: boolean
  /** Optional prefill (drag-drop path or deep-link URL); auto-loads the manifest. */
  initialSource?: string
}>()
const emit = defineEmits<{ 'update:visible': [boolean] }>()

const router = useRouter()
const { t, locale } = useI18n()
const store = useLauncherStore()

const source = ref('')
const loading = ref(false)
const manifest = ref<ModpackManifest | null>(null)
const instanceName = ref('')
const profileName = ref('')
const force = ref(false)
const busy = ref(false)

const canConfirm = computed(
  () => !!manifest.value && instanceName.value.trim().length > 0 && !busy.value,
)

watch(
  () => props.visible,
  async (v) => {
    if (!v) return
    manifest.value = null
    source.value = props.initialSource ?? ''
    force.value = false
    if (source.value) await loadManifest()
  },
)

/** Localized display name: string passthrough, or locale map with fallback. */
function localizedDisplayName(m: ModpackManifest): string | null {
  const d = m.displayName
  if (!d) return null
  if (typeof d === 'string') return d
  const map = d as Record<string, string>
  return map[locale.value] ?? map['en-US'] ?? Object.values(map)[0] ?? null
}

async function loadManifest() {
  if (!source.value.trim()) return
  loading.value = true
  manifest.value = null
  try {
    const m = await api.readModpackManifest(source.value.trim())
    manifest.value = m
    instanceName.value = localizedDisplayName(m) ?? m.name
    profileName.value = m.profileName?.trim() || 'pack'
  } catch (e) {
    Message.error(String(e))
  } finally {
    loading.value = false
  }
}

async function pickFile() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const file = await open({
    multiple: false,
    filters: [{ name: 'DSH Modpack', extensions: ['tgz'] }],
  })
  if (typeof file === 'string') {
    source.value = file
    await loadManifest()
  }
}

async function confirm() {
  if (!canConfirm.value) return
  busy.value = true
  try {
    await api.startImportModpackTask({
      source: source.value.trim(),
      force: force.value,
      instance_name: instanceName.value.trim(),
      profile_name: profileName.value.trim() || undefined,
    })
    emit('update:visible', false)
    await store.refreshTasks()
    Message.success(t('download.taskAdded'))
    router.push({ name: 'tasks' })
  } catch (e) {
    Message.error(String(e))
  } finally {
    busy.value = false
  }
}

function close() {
  emit('update:visible', false)
}
</script>

<template>
  <a-modal
    :visible="visible"
    :title="t('modpack.importTitle')"
    :ok-loading="busy"
    :ok-button-props="{ disabled: !canConfirm }"
    @ok="confirm"
    @cancel="close"
  >
    <a-form :model="{ source, instanceName, profileName }" layout="vertical">
      <a-form-item :label="t('modpack.source')" required>
        <a-input
          v-model="source"
          :placeholder="t('modpack.sourceHint')"
          allow-clear
          @press-enter="loadManifest"
        >
          <template #append>
            <a-button @click="pickFile">{{ t('modpack.pickFile') }}</a-button>
          </template>
        </a-input>
      </a-form-item>
      <a-form-item>
        <a-button size="small" :loading="loading" :disabled="!source.trim()" @click="loadManifest">
          {{ t('modpack.load') }}
        </a-button>
      </a-form-item>

      <template v-if="manifest">
        <a-alert type="info" class="modpack-summary">
          {{ manifest.name }} v{{ manifest.version }}
          <template v-if="manifest.author"> · {{ manifest.author }}</template>
          <template v-if="manifest.dshVersion"> · DSH {{ manifest.dshVersion }}</template>
        </a-alert>
        <a-form-item :label="t('modpack.instanceName')" required>
          <a-input v-model="instanceName" />
        </a-form-item>
        <a-form-item :label="t('modpack.profileName')">
          <a-input v-model="profileName" :placeholder="'pack'" />
        </a-form-item>
        <a-form-item>
          <a-checkbox v-model="force">{{ t('modpack.force') }}</a-checkbox>
        </a-form-item>
      </template>
    </a-form>
  </a-modal>
</template>

<style scoped>
.modpack-summary {
  margin-bottom: 12px;
}
</style>
