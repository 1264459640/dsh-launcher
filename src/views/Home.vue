<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Message } from '@arco-design/web-vue'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import { api } from '@/api'
import { useLauncherStore } from '@/stores/launcher'

// News links always open in a new window (the system browser on desktop)
// instead of navigating the launcher itself.
DOMPurify.addHook('afterSanitizeAttributes', (node) => {
  if (node.tagName === 'A') {
    node.setAttribute('target', '_blank')
    node.setAttribute('rel', 'noopener noreferrer')
  }
})

/** Renders md (GFM + inline HTML) or raw HTML, sanitized against XSS. */
function renderNews(content: string, source: string): string {
  const isHtml = /\.html?([?#].*)?$/i.test(source)
  const raw = isHtml ? content : (marked.parse(content, { gfm: true, breaks: true }) as string)
  return DOMPurify.sanitize(raw, {
    USE_PROFILES: { html: true },
    FORBID_TAGS: ['style', 'iframe', 'object', 'embed', 'form', 'input', 'textarea', 'select', 'button'],
    FORBID_ATTR: ['srcset'],
  })
}

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

// On mount the instance id may already be restored from settings without a
// watch change (e.g. navigating back to this page) — load profiles eagerly.
onMounted(() => {
  if (selectedInstanceId.value) loadProfiles()
  loadNews()
})

// --- News area ---------------------------------------------------------------

const newsSource = computed(() => (store.settings.news_source ?? '').trim())
const newsHtml = ref('')
const newsLoading = ref(false)
const newsError = ref('')

async function loadNews() {
  const src = newsSource.value
  newsHtml.value = ''
  newsError.value = ''
  if (!src) return
  newsLoading.value = true
  try {
    const content = await api.fetchNews(src)
    newsHtml.value = renderNews(content, src)
  } catch (e) {
    newsError.value = String(e)
  } finally {
    newsLoading.value = false
  }
}

watch(newsSource, () => loadNews())

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
        <div class="instance-avatar"><img src="@/assets/launcher-icon.png" alt="" /></div>
        <div class="instance-name">{{ selectedInstance?.name ?? '—' }}</div>
        <a-tag
          v-if="selectedStatus"
          :color="selectedStatus.state === 'running' ? 'green' : selectedStatus.state === 'starting' ? 'orange' : 'gray'"
          size="small"
        >
          {{ t(`home.status.${selectedStatus.state}`) }}
        </a-tag>
        <div v-if="running && selectedStatus?.url" class="running-url">
          <a-link @click="onOpenWindow">{{ selectedStatus.url }}</a-link>
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

    <!-- Right news area: renders the configured md/html source (XSS-sanitized) -->
    <section class="news-area">
      <div v-if="!newsSource" class="news-placeholder">{{ t('home.newsPlaceholder') }}</div>
      <div v-else-if="newsLoading" class="news-placeholder">
        <a-spin :size="20" />
      </div>
      <div v-else-if="newsError" class="news-placeholder news-error">
        <span>{{ newsError }}</span>
        <a-button size="mini" @click="loadNews">{{ t('common.refresh') }}</a-button>
      </div>
      <a-scrollbar v-else outer-style="height: 100%" style="height: 100%; overflow-y: auto">
        <article class="news-body" v-html="newsHtml"></article>
      </a-scrollbar>
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
  overflow: hidden;
  box-shadow: 0 6px 16px rgb(22 93 255 / 25%);
  user-select: none;

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
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
  flex: 1;
}

.news-area {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  background:
    radial-gradient(circle at 30% 20%, rgb(22 93 255 / 6%), transparent 40%),
    radial-gradient(circle at 70% 80%, rgb(114 46 209 / 6%), transparent 40%);
}

.news-placeholder {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--color-text-4);
  font-size: 14px;
  letter-spacing: 4px;
  user-select: none;
}

.news-error {
  color: rgb(var(--red-6));
  font-size: 13px;
  letter-spacing: 0;
  padding: 0 24px;
  text-align: center;
  word-break: break-all;
}

.news-body {
  padding: 24px 28px;
  font-size: 14px;
  line-height: 1.7;
  color: var(--color-text-1);
  word-wrap: break-word;

  :deep(h1),
  :deep(h2),
  :deep(h3) {
    margin: 18px 0 10px;
    line-height: 1.35;
  }

  :deep(h1) {
    font-size: 22px;
  }

  :deep(h2) {
    font-size: 18px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--color-border-2);
  }

  :deep(h3) {
    font-size: 16px;
  }

  :deep(p) {
    margin: 8px 0;
  }

  :deep(a) {
    color: rgb(var(--primary-6));
    text-decoration: none;

    &:hover {
      text-decoration: underline;
    }
  }

  :deep(code) {
    font-family: Consolas, 'Courier New', monospace;
    font-size: 0.9em;
    background: var(--color-fill-2);
    border-radius: 4px;
    padding: 1px 5px;
  }

  :deep(pre) {
    background: #1d2129;
    color: #a9b7c6;
    border-radius: 8px;
    padding: 12px 16px;
    overflow-x: auto;
    margin: 12px 0;

    code {
      background: none;
      padding: 0;
    }
  }

  :deep(blockquote) {
    margin: 12px 0;
    padding: 4px 14px;
    border-left: 3px solid rgb(var(--primary-6));
    background: var(--color-fill-1);
    color: var(--color-text-2);
    border-radius: 0 6px 6px 0;
  }

  :deep(table) {
    border-collapse: collapse;
    margin: 12px 0;
    max-width: 100%;
    display: block;
    overflow-x: auto;

    th,
    td {
      border: 1px solid var(--color-border-2);
      padding: 6px 12px;
      font-size: 13px;
    }

    th {
      background: var(--color-fill-2);
      font-weight: 600;
    }
  }

  :deep(ul),
  :deep(ol) {
    margin: 8px 0;
    padding-left: 24px;
  }

  :deep(li) {
    margin: 4px 0;
  }

  :deep(img) {
    max-width: 100%;
    border-radius: 6px;
  }

  :deep(hr) {
    border: none;
    border-top: 1px solid var(--color-border-2);
    margin: 16px 0;
  }
}

.option-line {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
</style>
