<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
</script>

<template>
  <div class="dl-page">
    <div class="dl-card coming-card">
      <div class="coming-text">{{ t('plugins.coming') }}</div>
      <div class="coming-desc">{{ t('plugins.desc') }}</div>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.coming-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 420px;
  gap: 16px;
}

.coming-text {
  font-size: 44px;
  font-weight: 800;
  letter-spacing: 1px;
  background: linear-gradient(90deg, #165dff, #722ed1, #f53f3f);
  background-clip: text;
  -webkit-text-fill-color: transparent;
  user-select: none;
}

.coming-desc {
  color: var(--color-text-3);
  font-size: 14px;
}
</style>
