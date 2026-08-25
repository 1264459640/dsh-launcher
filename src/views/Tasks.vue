<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Message } from '@arco-design/web-vue'
import { api } from '@/api'
import { useLauncherStore } from '@/stores/launcher'
import type { TaskInfo, TaskState } from '@/api/types'

const router = useRouter()
const { t } = useI18n()
const store = useLauncherStore()

const expanded = ref<Record<string, boolean>>({})
const logRefs = ref<Record<string, HTMLElement | null>>({})

function stateColor(state: TaskState): string {
  switch (state) {
    case 'running':
      return 'orange'
    case 'done':
      return 'green'
    case 'error':
      return 'red'
    case 'cancelled':
      return 'gray'
  }
}

function setLogRef(id: string, el: unknown) {
  logRefs.value[id] = (el as HTMLElement | null) ?? null
}

async function scrollToBottom(id: string) {
  await nextTick()
  const el = logRefs.value[id]
  if (el) el.scrollTop = el.scrollHeight
}

watch(
  () => store.tasks,
  () => {
    for (const task of store.taskList) {
      if (expanded.value[task.id]) scrollToBottom(task.id)
    }
  },
  { deep: true },
)

function toggleExpand(id: string) {
  expanded.value[id] = !expanded.value[id]
  if (expanded.value[id]) scrollToBottom(id)
}

async function onCancel(id: string) {
  try {
    await api.cancelTask(id)
  } catch (e) {
    Message.error(String(e))
  }
}

async function onRemove(id: string) {
  try {
    await api.removeTask(id)
    await store.refreshTasks()
  } catch (e) {
    Message.error(String(e))
  }
}

function formatTime(ms: number): string {
  return new Date(ms).toLocaleString()
}

function instanceName(task: TaskInfo): string | null {
  if (!task.instance_id) return null
  return store.instanceById(task.instance_id)?.name ?? null
}

const sortedTasks = computed(() => store.taskList)
</script>

<template>
  <div class="dl-page">
    <div class="dl-card">
      <div class="dl-card-title">
        <h3>{{ t('tasks.title') }}</h3>
        <div class="dl-toolbar">
          <a-button size="small" @click="store.refreshTasks()">{{ t('common.refresh') }}</a-button>
          <a-button size="small" type="text" @click="router.back()">{{ t('download.back') }}</a-button>
        </div>
      </div>

      <div v-if="sortedTasks.length === 0" class="empty-tasks">
        <a-empty :description="t('tasks.empty')" />
      </div>

      <div v-for="task in sortedTasks" :key="task.id" class="task-card">
        <div class="task-head" @click="toggleExpand(task.id)">
          <div class="task-info">
            <div class="task-label">
              {{ task.label }}
              <a-tag :color="stateColor(task.state)" size="small">{{ t(`tasks.state.${task.state}`) }}</a-tag>
            </div>
            <div class="task-meta">
              {{ formatTime(task.created_at) }}
              <template v-if="task.state === 'done' && instanceName(task)">
                · {{ t('tasks.createdInstance', { name: instanceName(task) }) }}
              </template>
              <template v-if="task.state === 'error' && task.message"> · {{ task.message }}</template>
            </div>
          </div>
          <div class="task-progress">
            <a-progress
              v-if="task.state === 'running'"
              :percent="task.percent / 100"
              size="small"
              :show-text="true"
            />
            <a-progress
              v-else-if="task.state === 'done'"
              :percent="1"
              size="small"
              status="success"
            />
            <a-progress
              v-else
              :percent="task.percent / 100"
              size="small"
              :status="task.state === 'error' ? 'danger' : 'normal'"
            />
          </div>
          <div class="task-actions" @click.stop>
            <a-button v-if="task.state === 'running'" size="mini" status="warning" @click="onCancel(task.id)">
              {{ t('tasks.cancel') }}
            </a-button>
            <a-button v-else size="mini" status="danger" @click="onRemove(task.id)">
              {{ t('tasks.remove') }}
            </a-button>
          </div>
          <span class="expand-icon">{{ expanded[task.id] ? '▾' : '▸' }}</span>
        </div>

        <div v-if="expanded[task.id]" class="task-log" :ref="(el) => setLogRef(task.id, el)">
          <pre><code>{{ task.logs.join('\n') || t('tasks.noLogs') }}</code></pre>
        </div>
      </div>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.empty-tasks {
  padding: 40px 0;
}

.task-card {
  border: 1px solid var(--color-border-2);
  border-radius: 8px;
  margin-bottom: 12px;
  overflow: hidden;
}

.task-head {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 12px 16px;
  cursor: pointer;
  user-select: none;

  &:hover {
    background: var(--color-fill-1);
  }
}

.task-info {
  flex: 1;
  min-width: 0;
}

.task-label {
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 8px;
}

.task-meta {
  font-size: 12px;
  color: var(--color-text-3);
  margin-top: 4px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.task-progress {
  width: 180px;
  flex-shrink: 0;
}

.task-actions {
  flex-shrink: 0;
}

.expand-icon {
  color: var(--color-text-3);
  width: 16px;
}

.task-log {
  background: #1d2129;
  max-height: 280px;
  overflow-y: auto;
  border-top: 1px solid var(--color-border-2);

  pre {
    margin: 0;
    padding: 12px 16px;
    font-size: 12px;
    line-height: 1.6;
    color: #a9b7c6;
    font-family: Consolas, 'Courier New', monospace;
    white-space: pre-wrap;
    word-break: break-all;
  }
}
</style>
