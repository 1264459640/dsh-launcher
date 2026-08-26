<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { api } from '@/api'
import { useLauncherStore } from '@/stores/launcher'
import type { MarketPlugin, PluginChannel, PluginVersionInfo } from '@/api/types'

const router = useRouter()
const { t } = useI18n()
const store = useLauncherStore()

const plugin = computed<MarketPlugin | null>(() => store.pluginWizard?.plugin ?? null)

const channels: PluginChannel[] = ['stable', 'beta', 'alpha']

const channelMeta: Record<PluginChannel, { letter: string; color: string }> = {
  stable: { letter: 'R', color: 'green' },
  beta: { letter: 'B', color: 'orange' },
  alpha: { letter: 'A', color: 'red' },
}

const versionsByChannel = ref<Record<PluginChannel, PluginVersionInfo[]>>({
  stable: [],
  beta: [],
  alpha: [],
})
const loadingChannel = ref<PluginChannel | null>(null)
const error = ref<string | null>(null)

async function loadChannel(ch: PluginChannel) {
  if (!plugin.value) return
  loadingChannel.value = ch
  error.value = null
  try {
    const list = await api.fetchPluginVersions(plugin.value.id, ch)
    versionsByChannel.value[ch] = list
  } catch (e) {
    error.value = String(e)
  } finally {
    loadingChannel.value = null
  }
}

onMounted(() => {
  if (!plugin.value) {
    // No plugin carried over: go back to the market.
    router.replace({ name: 'download-plugins' })
    return
  }
  // Load stable and beta lazily; alpha needs the repo so it is loaded on demand.
  loadChannel('stable')
  loadChannel('beta')
})

function pick(ch: PluginChannel, v: PluginVersionInfo) {
  store.pluginWizard = { plugin: plugin.value!, channel: ch, version: v }
  router.push({ name: 'plugin-install' })
}

function formatLabel(v: PluginVersionInfo): string {
  return v.label ?? v.version
}

const hasAny = computed(() =>
  channels.some((ch) => versionsByChannel.value[ch].length > 0),
)
</script>

<template>
  <div class="version-pick">
    <a-page-header
      class="pick-header"
      :title="plugin?.name ?? t('plugins.chooseVersion')"
      :sub-title="plugin?.id"
      @back="router.push({ name: 'download-plugins' })"
    />

    <div v-if="error" class="pick-error">
      <a-alert :title="error" type="error" />
    </div>

    <div v-if="!plugin" class="pick-empty">
      <a-empty :description="t('plugins.noMatch')" />
    </div>

    <template v-else>
      <div v-for="ch in channels" :key="ch" class="dl-card channel-card">
        <div class="dl-card-title">
          <h3>
            <span
              class="channel-letter"
              :style="{ background: channelMeta[ch].color }"
            >{{ channelMeta[ch].letter }}</span>
            {{ t(`plugins.channel.${ch}`) }}
            <span class="channel-desc">{{ t(`plugins.channelDesc.${ch}`) }}</span>
          </h3>
          <a-button
            size="small"
            type="text"
            :loading="loadingChannel === ch"
            @click="loadChannel(ch)"
          >
            ⟳
          </a-button>
        </div>

        <template v-if="versionsByChannel[ch].length">
          <div
            v-for="v in versionsByChannel[ch]"
            :key="v.version"
            class="version-row"
            @click="pick(ch, v)"
          >
            <span
              class="version-icon"
              :style="{ background: channelMeta[ch].color }"
            >{{ channelMeta[ch].letter }}</span>
            <div class="version-meta">
              <div class="version-name">
                {{ v.version }}
                <a-tag v-if="v.is_default" size="small" color="green">
                  {{ t('plugins.defaultTag') }}
                </a-tag>
              </div>
              <div class="version-sub">{{ formatLabel(v) }}</div>
            </div>
            <span class="version-arrow">›</span>
          </div>
        </template>
        <div v-else class="card-empty">{{ t('plugins.noVersions') }}</div>
      </div>

      <a-empty v-if="!hasAny && !loadingChannel" :description="t('plugins.noVersions')" />
    </template>
  </div>
</template>

<style lang="scss" scoped>
.version-pick {
  max-width: 860px;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.pick-header {
  padding: 0;
  background: transparent;
  border-radius: 8px;
}

.pick-error {
  margin-bottom: 4px;
}

.pick-empty {
  padding: 40px 0;
}

.channel-letter {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border-radius: 6px;
  color: #fff;
  font-size: 13px;
  font-weight: 700;
  margin-right: 6px;
  vertical-align: -3px;
}

.channel-desc {
  font-size: 12px;
  font-weight: 400;
  color: var(--color-text-3);
  margin-left: 8px;
}

.version-row {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 10px 12px;
  border-radius: 8px;
  cursor: pointer;
  transition: background 0.15s;

  &:hover {
    background: var(--color-fill-2);
  }
}

.version-icon {
  width: 40px;
  height: 40px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #fff;
  font-size: 16px;
  font-weight: 700;
  flex-shrink: 0;
}

.version-meta {
  flex: 1;
  min-width: 0;
}

.version-name {
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 8px;
}

.version-sub {
  font-size: 12px;
  color: var(--color-text-3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.version-arrow {
  color: var(--color-text-3);
  font-size: 20px;
}

.card-empty {
  padding: 18px 12px;
  color: var(--color-text-3);
  font-size: 13px;
}
</style>
