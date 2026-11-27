<script setup lang="ts">
import { useWallpaper } from '@/composables/wallpaper';
import { Pages, useGlobalState } from '@/composables/state';
import IconRustLang from './IconRustLang.vue';
import IconVueJs from './IconVueJs.vue';
import UpdateNotice from './UpdateNotice.vue';
import { getVersion } from '@tauri-apps/api/app';
import { onMounted, shallowRef } from 'vue';

const appState = useGlobalState();
const { page, setPage, t } = appState;
const { image: wallpaper, style: wallpaperStyle } = useWallpaper();
const version = shallowRef('');
const releaseDate = shallowRef('');
onMounted(async () => {
  try {
    version.value = await getVersion();
    const [year, month, patch] = version.value.split('.').map(Number);
    releaseDate.value = `${String(Math.floor(patch / 100)).padStart(2, '0')}.${String(month).padStart(2, '0')}.${year}`;
  } catch { /* The main interface remains usable if version metadata is unavailable. */ }
});
</script>

<template>
  <div class="flex flex-col h-dvh overflow-hidden bg-gray-100 dark:bg-gray-900">
    <header class="shrink-0 bg-white dark:bg-gray-800 shadow-md">
      <div class="app-frame flex items-center justify-between gap-5 py-4">
        <div class="flex shrink-0 items-center space-x-2">
          <img src="/logo.svg" alt="Discord Quest Helper" class="h-8 w-8" />
          <h2 class="text-xl font-bold text-gray-900 dark:text-white"></h2>
        </div>
        <nav class="min-w-0 flex-1 overflow-x-auto">
          <ul class="flex min-w-max justify-end gap-x-3 whitespace-nowrap text-sm">
            <li>
              <a href="#"
                class="text-gray-700 dark:text-gray-300 hover:text-indigo-600 dark:hover:text-indigo-400"
                :class="{ 'text-indigo-600 dark:text-indigo-400': page === Pages.HOME }"
                @click.prevent="setPage(Pages.HOME)"
              >
                {{ t('nav.home') }}
              </a>
            </li>
            <li>
              <a href="#"
                class="text-gray-700 dark:text-gray-300 hover:text-indigo-600 dark:hover:text-indigo-400"
                :class="{ 'text-indigo-600 dark:text-indigo-400': page === Pages.ACTIVE_QUESTS }"
                @click.prevent="setPage(Pages.ACTIVE_QUESTS)"
              >
                {{ t('nav.activeQuests') }}
              </a>
            </li>
            <li>
              <a href="#"
                class="text-gray-700 dark:text-gray-300 hover:text-indigo-600 dark:hover:text-indigo-400"
                :class="{ 'text-indigo-600 dark:text-indigo-400': page === Pages.GAMES }"
                @click.prevent="setPage(Pages.GAMES)"
              >
                {{ t('nav.games') }}
              </a>
            </li>
            <li>
              <a href="#"
                class="text-gray-700 dark:text-gray-300 hover:text-indigo-600 dark:hover:text-indigo-400"
                :class="{ 'text-indigo-600 dark:text-indigo-400': page === Pages.BULK_CREATE }"
                @click.prevent="setPage(Pages.BULK_CREATE)"
              >
                {{ t('nav.bulkCreate') }}
              </a>
            </li>
            <li>
              <a href="#"
                class="text-gray-700 dark:text-gray-300 hover:text-indigo-600 dark:hover:text-indigo-400"
                :class="{ 'text-indigo-600 dark:text-indigo-400': page === Pages.GENERATED_GAMES }"
                @click.prevent="setPage(Pages.GENERATED_GAMES)"
              >
                {{ t('nav.generatedGames') }}
              </a>
            </li>
            <li>
              <a href="#"
                class="text-gray-700 dark:text-gray-300 hover:text-indigo-600 dark:hover:text-indigo-400"
                :class="{ 'text-indigo-600 dark:text-indigo-400': page === Pages.PLAYGROUND }"
                @click.prevent="setPage(Pages.PLAYGROUND)"
              >
                {{ t('nav.playground') }}
              </a>
            </li>
            <li>
              <a href="#"
                class="text-gray-700 dark:text-gray-300 hover:text-indigo-600 dark:hover:text-indigo-400"
                :class="{ 'text-indigo-600 dark:text-indigo-400': page === Pages.SETTINGS }"
                @click.prevent="setPage(Pages.SETTINGS)"
              >
                {{ t('nav.settings') }}
              </a>
            </li>
          </ul>
        </nav>
      </div>
    </header>

    <UpdateNotice />
    <div class="relative flex min-h-0 flex-1 flex-col">
      <div v-if="wallpaper" aria-hidden="true" class="pointer-events-none absolute inset-0 overflow-hidden">
        <img :src="wallpaper" :style="wallpaperStyle" alt="" class="absolute h-auto max-w-none" />
      </div>
      <main class="relative min-h-0 flex-1 overflow-y-auto" :class="{ 'wallpaper-content': wallpaper }">
        <slot></slot>
      </main>
    </div>

    <footer class="shrink-0 bg-white dark:bg-gray-800 border-t border-gray-200 dark:border-gray-700 mt-auto">
      <div class="app-frame py-4 text-center text-sm text-gray-600 dark:text-gray-400">
        <span v-if="version">v{{ version }} · {{ releaseDate }} · </span>
        <span>{{ t('footer.builtWith') }}</span>
        <IconRustLang class="inline-block h-4 w-4 text-red-200 dark:text-white mx-0.5" />
        <span>Rust, Tauri,</span>
        <IconVueJs class="h-4 w-4 inline-block text-[#4FC08D] mx-0.5" />
        <span>Vue.js, TypeScript {{ t('footer.and') }} JavaScript</span>
      </div>
    </footer>
  </div>
</template>
