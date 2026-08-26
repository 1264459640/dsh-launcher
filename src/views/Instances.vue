<script setup lang="ts">
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Message } from '@arco-design/web-vue'
import { api } from '@/api'
import { useLauncherStore } from '@/stores/launcher'
import type { DshInstance, InstanceState } from '@/api/types'

const router = useRouter()
const { t } = useI18n()
const store = useLauncherStore()

const columns = computed(() => [
  { title: t('instances.table.name'), dataIndex: 'name', width: 180 },
  { title: t('instances.table.version'), slotName: 'version', width: 140 },
  { title: t('instances.table.home'), slotName: 'home', width: 180 },
  { title: t('instances.table.profile'), slotName: 'profile', width: 120 },
  { title: t('instances.table.status'), slotName: 'status' },
  { title: t('instances.table.actions'), slotName: 'actions', width: 200, align: 'center' as const },
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

// --- Copy instance ---------------------------------------------------------

const copySource = ref<DshInstance | null>(null)
const copyName = ref('')
const copyNewHome = ref(false)
const copying = ref(false)

function openCopy(inst: DshInstance) {
  copySource.value = inst
  copyName.value = `${inst.name} 副本`
  copyNewHome.value = false
}

function closeCopy() {
  copySource.value = null
  copyName.value = ''
  copyNewHome.value = false
}

const copyValid = computed(() => copyName.value.trim().length > 0 && !!copySource.value)

async function confirmCopy() {
  if (!copySource.value || !copyValid.value) return
  copying.value = true
  try {
    const created = await api.copyInstance({
      source_id: copySource.value.id,
      name: copyName.value.trim(),
      new_home: copyNewHome.value,
    })
    await store.refreshInstances()
    Message.success(t('instances.copied', { name: created.name }))
    closeCopy()
  } catch (e) {
    Message.error(String(e))
  } finally {
    copying.value = false
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
            <a-button size="small" @click="openCopy(record)">
              {{ t('instances.table.copy') }}
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

    <!-- Copy instance dialog: name it first, then duplicate on save -->
    <a-modal
      :visible="!!copySource"
      :title="t('instances.copyTitle')"
      :ok-text="t('instanceEdit.save')"
      :cancel-text="t('instanceEdit.cancel')"
      :ok-button-props="{ disabled: !copyValid, loading: copying }"
      @ok="confirmCopy"
      @cancel="closeCopy"
    >
      <a-form layout="vertical" :model="{}">
        <a-form-item :label="t('instances.copyNameLabel')" required>
          <a-input v-model="copyName" :placeholder="t('instances.copyNamePlaceholder')" />
        </a-form-item>
        <a-form-item :label="t('instances.copyHomeLabel')">
          <a-radio-group v-model="copyNewHome" type="button">
            <a-radio :value="false">{{ t('instances.copyHomeReuse') }}</a-radio>
            <a-radio :value="true">{{ t('instances.copyHomeNew') }}</a-radio>
          </a-radio-group>
        </a-form-item>
        <p class="copy-hint">{{ t('instances.copyHint') }}</p>
      </a-form>
    </a-modal>
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

.copy-hint {
  margin: 0;
  font-size: 12px;
  color: var(--color-text-3);
}
</style>
