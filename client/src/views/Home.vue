<template>
  <div class="home">
    <section class="hero is-primary is-small">
      <div class="hero-body">
        <div class="container">
          <h1 class="title">
            <IconFiles :size="32" class="mr-2" />
            VFiles
          </h1>
          <p class="subtitle">基于Git的文件管理系统</p>
        </div>
      </div>
    </section>

    <section class="section">
      <div class="container">
        <FileBrowser />
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { IconFiles } from '@tabler/icons-vue';
import FileBrowser from '../components/file-browser/FileBrowser.vue';
</script>

<style scoped>
.home {
  min-height: 100vh;
  background: #f5f5f5;
}

.hero {
  margin-bottom: 0;
}

@media screen and (max-width: 768px) {
  .section {
    padding: 1rem 0.5rem;
  }
}
</style>
