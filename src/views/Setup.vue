<script setup lang="ts">
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Message } from '@arco-design/web-vue'
import { api } from '@/api'
import { useLauncherStore } from '@/stores/launcher'

const router = useRouter()
const { t } = useI18n()
const store = useLauncherStore()

const checking = ref(false)

const nodeOk = computed(() => store.runtime?.node?.installed ?? false)
const pnpmOk = computed(() => store.runtime?.pnpm?.installed ?? false)
const allOk = computed(() => nodeOk.value && pnpmOk.value)

const nodeVersion = computed(() => store.runtime?.node?.version ?? '')
const pnpmVersion = computed(() => store.runtime?.pnpm?.version ?? '')

async function recheck() {
  checking.value = true
  try {
    await store.checkRuntime()
    if (store.runtime?.node?.installed && store.runtime?.pnpm?.installed) {
      Message.success(t('setup.allReady'))
      router.push({ name: 'home' })
    }
  } finally {
    checking.value = false
  }
}
</script>

<template>
  <div class="setup-page">
    <div class="dl-card setup-card">
      <div class="setup-icon">🛠️</div>
      <h2>{{ t('setup.title') }}</h2>
      <p class="setup-desc">{{ t('setup.desc') }}</p>

      <!-- Node status -->
      <div class="tool-row">
        <span class="tool-name">Node.js</span>
        <a-tag v-if="nodeOk" color="green">{{ t('setup.installed', { v: nodeVersion }) }}</a-tag>
        <a-tag v-else color="red">{{ t('setup.missing') }}</a-tag>
      </div>

      <!-- pnpm status -->
      <div class="tool-row">
        <span class="tool-name">pnpm</span>
        <a-tag v-if="pnpmOk" color="green">{{ t('setup.installed', { v: pnpmVersion }) }}</a-tag>
        <a-tag v-else color="red">{{ t('setup.missing') }}</a-tag>
      </div>

      <!-- Guidance -->
      <div v-if="!nodeOk" class="guide-block">
        <h4>{{ t('setup.installNode') }}</h4>
        <ol>
          <li>{{ t('setup.nodeStep1') }}</li>
          <li>{{ t('setup.nodeStep2') }}</li>
          <li>{{ t('setup.nodeStep3') }}</li>
        </ol>
        <a-button type="primary" @click="api.openExternal('https://nodejs.org/zh-cn/download')">
          {{ t('setup.openNodeSite') }}
        </a-button>
      </div>

      <div v-if="nodeOk && !pnpmOk" class="guide-block">
        <h4>{{ t('setup.installPnpm') }}</h4>
        <p>{{ t('setup.pnpmAuto') }}</p>
      </div>

      <div v-if="allOk" class="guide-block">
        <a-result status="success" :title="t('setup.allReady')" />
        <a-button type="primary" @click="router.push({ name: 'home' })">
          {{ t('setup.enterApp') }}
        </a-button>
      </div>

      <div class="setup-actions">
        <a-button :loading="checking" type="outline" @click="recheck">
          {{ t('setup.recheck') }}
        </a-button>
      </div>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.setup-page {
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
}

.setup-card {
  max-width: 560px;
  width: 100%;
  text-align: center;
  padding: 40px 48px;
}

.setup-icon {
  font-size: 44px;
}

h2 {
  margin: 12px 0 4px;
}

.setup-desc {
  color: var(--color-text-3);
  margin-bottom: 24px;
}

.tool-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 0;
  border-bottom: 1px dashed var(--color-border-2);

  .tool-name {
    font-weight: 600;
  }
}

.guide-block {
  text-align: left;
  margin-top: 24px;
  padding: 16px;
  background: var(--color-fill-1);
  border-radius: 8px;

  h4 {
    margin: 0 0 8px;
  }

  ol {
    padding-left: 20px;
    line-height: 1.8;
    color: var(--color-text-2);
  }
}

.code-block {
  background: #1d2129;
  color: #a9b7c6;
  padding: 10px 14px;
  border-radius: 6px;
  font-family: Consolas, 'Courier New', monospace;
  user-select: text;
}

.setup-actions {
  margin-top: 24px;
}
</style>