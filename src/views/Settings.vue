<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Message } from '@arco-design/web-vue'
import { api } from '@/api'
import { SUPPORTED_LOCALES } from '@/i18n'
import { useLauncherStore } from '@/stores/launcher'

const { t } = useI18n()
const store = useLauncherStore()

// --- General settings -------------------------------------------------------

async function patchSettings(patch: Parameters<typeof api.updateSettings>[0]) {
  try {
    store.settings = await api.updateSettings(patch)
    Message.success(t('settings.saved'))
  } catch (e) {
    Message.error(String(e))
  }
}

async function onLocaleChange(value: string | number | boolean | Record<string, unknown> | (string | number | boolean | Record<string, unknown>)[]) {
  await patchSettings({ locale: String(value) })
}

async function onTrayChange(value: string | number | boolean | Record<string, unknown> | (string | number | boolean | Record<string, unknown>)[]) {
  await patchSettings({ minimize_to_tray: Boolean(value) })
}

async function onAutostartChange(value: string | number | boolean | Record<string, unknown> | (string | number | boolean | Record<string, unknown>)[]) {
  await patchSettings({ autostart: Boolean(value) })
}

// --- DSH_HOME management ------------------------------------------------------

const newHomeName = ref('')
const newHomePath = ref('')

async function onPickDir() {
  if (api.isTauri) {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog')
      const dir = await open({ directory: true, multiple: false })
      if (typeof dir === 'string') newHomePath.value = dir
    } catch (e) {
      Message.error(String(e))
    }
  } else {
    Message.info(t('settings.browserPickHint'))
  }
}

async function onAddHome() {
  try {
    await api.createHome(newHomeName.value, newHomePath.value)
    newHomeName.value = ''
    newHomePath.value = ''
    await store.refreshHomes()
    Message.success(t('settings.saved'))
  } catch (e) {
    Message.error(String(e))
  }
}

async function onRemoveHome(id: string) {
  try {
    await api.removeHome(id)
    await store.refreshHomes()
  } catch (e) {
    Message.error(String(e))
  }
}

const homeColumns = computed(() => [
  { title: t('settings.homeName'), dataIndex: 'name', width: 180 },
  { title: t('settings.homePath'), dataIndex: 'path', ellipsis: true, tooltip: true },
  { title: t('home.table.actions'), slotName: 'actions', width: 110, align: 'center' as const },
])
</script>

<template>
  <div class="dl-page">
    <div class="dl-card">
      <div class="dl-card-title">
        <h3>{{ t('settings.general') }}</h3>
      </div>
      <a-form :model="store.settings" layout="vertical" class="settings-form">
        <a-form-item :label="t('settings.language')">
          <a-select
            :model-value="store.settings.locale"
            style="width: 220px"
            @change="onLocaleChange"
          >
            <a-option v-for="l in SUPPORTED_LOCALES" :key="l.value" :value="l.value">
              {{ l.label }}
            </a-option>
          </a-select>
        </a-form-item>
        <a-form-item>
          <a-switch :model-value="store.settings.minimize_to_tray" @change="onTrayChange" />
          <span class="switch-label">{{ t('settings.minimizeToTray') }}</span>
        </a-form-item>
        <a-form-item>
          <a-switch :model-value="store.settings.autostart" @change="onAutostartChange" />
          <span class="switch-label">{{ t('settings.autostart') }}</span>
        </a-form-item>
      </a-form>
    </div>

    <div class="dl-card">
      <div class="dl-card-title">
        <h3>{{ t('settings.homes') }}</h3>
      </div>

      <div class="home-add-row">
        <a-input v-model="newHomeName" :placeholder="t('settings.homeNamePlaceholder')" style="width: 200px" />
        <a-input v-model="newHomePath" :placeholder="t('settings.homePathPlaceholder')" class="home-path-input" />
        <a-button @click="onPickDir">{{ t('settings.pickDir') }}</a-button>
        <a-button type="primary" :disabled="!newHomeName.trim() || !newHomePath.trim()" @click="onAddHome">
          {{ t('settings.addHome') }}
        </a-button>
      </div>

      <a-table :columns="homeColumns" :data="store.homes" :pagination="false" row-key="id">
        <template #actions="{ record }">
          <a-popconfirm
            :content="t('settings.confirmDeleteHome', { name: record.name })"
            @ok="onRemoveHome(record.id)"
          >
            <a-button size="small" status="danger">{{ t('settings.deleteHome') }}</a-button>
          </a-popconfirm>
        </template>
      </a-table>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.settings-form {
  max-width: 560px;
}

.switch-label {
  margin-left: 10px;
  color: var(--color-text-2);
}

.home-add-row {
  display: flex;
  gap: 8px;
  margin-bottom: 16px;
}

.home-path-input {
  flex: 1;
}
</style>
