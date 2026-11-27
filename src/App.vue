<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, type Component } from 'vue';
import { useCloseGames } from './composables/close-games';
import MainLayout from './components/MainLayout.vue';
import { Pages, useGlobalState } from './composables/state';
import BulkCreateView from './pages/BulkCreateView.vue';
import GamesView from './pages/GamesView.vue';
import GeneratedGamesView from './pages/GeneratedGamesView.vue';
import ActiveQuestsView from './pages/ActiveQuestsView.vue';
import HomeView from './pages/HomeView.vue';
import Playground from './pages/Playground.vue';
import SettingsView from './pages/SettingsView.vue';
import { useNewGames } from './composables/new-games';
import { usePromoPresence } from './composables/promo-presence';

const appState = useGlobalState();
const { page } = appState;
const {
  newlyDetectedGames,
  isModalOpen: isNewGamesModalOpen,
  closeNewGamesModal,
} = useNewGames();
const { start: startPromoPresence } = usePromoPresence();
const visibleNewGames = computed(() => newlyDetectedGames.value.slice(0, 12));
const pageComponents: Record<Pages, Component> = {
  [Pages.HOME]: HomeView,
  [Pages.ACTIVE_QUESTS]: ActiveQuestsView,
  [Pages.GAMES]: GamesView,
  [Pages.BULK_CREATE]: BulkCreateView,
  [Pages.GENERATED_GAMES]: GeneratedGamesView,
  [Pages.PLAYGROUND]: Playground,
  [Pages.SETTINGS]: SettingsView,
};

const currentPageComponent = computed(() => pageComponents[page.value]);

const closeGames = useCloseGames();
onMounted(() => {
  void startPromoPresence();
  void closeGames.start();
});
onUnmounted(closeGames.dispose);

async function openNewGamesInSettings() {
  closeNewGamesModal();
  appState.setPage(Pages.SETTINGS);

  await nextTick();
  window.setTimeout(() => {
    document.getElementById('settings-new-games')?.scrollIntoView({
      behavior: 'smooth',
      block: 'start',
    });
  }, 50);
}

</script>

<template>
  <MainLayout>
    <KeepAlive>
      <component :is="currentPageComponent" v-if="currentPageComponent" :key="page" />
    </KeepAlive>
    <Transition
      enter-active-class="transition-opacity duration-150"
      leave-active-class="transition-opacity duration-150"
      enter-from-class="opacity-0"
      leave-to-class="opacity-0"
    >
      <div
        v-if="isNewGamesModalOpen && newlyDetectedGames.length > 0"
        class="fixed inset-0 z-[80] flex items-center justify-center bg-black/55 p-5 backdrop-blur-sm"
      >
        <div class="w-full max-w-lg rounded-lg border border-white/10 bg-white p-5 shadow-2xl dark:bg-gray-900">
          <div class="flex items-start justify-between gap-4">
            <div>
              <h2 class="text-xl font-bold text-gray-900 dark:text-white">
                {{ appState.t('newGames.modalTitle') }}
              </h2>
              <p class="mt-1 text-sm text-gray-500 dark:text-gray-400">
                {{ appState.t('newGames.modalDescription').replace('{count}', String(newlyDetectedGames.length)) }}
              </p>
            </div>
            <button
              type="button"
              class="rounded-md border border-gray-200 px-2 py-1 text-sm font-semibold text-gray-500 hover:bg-gray-100 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800"
              @click="closeNewGamesModal"
            >
              ×
            </button>
          </div>

          <div class="mt-4 max-h-72 space-y-2 overflow-y-auto pr-1">
            <div
              v-for="game in visibleNewGames"
              :key="game.id"
              class="rounded-md border border-gray-200 bg-gray-50 px-3 py-2 dark:border-gray-700 dark:bg-gray-800"
            >
              <div class="font-semibold text-gray-900 dark:text-white">
                {{ game.name }}
              </div>
              <div class="mt-1 text-xs text-gray-500 dark:text-gray-400">
                ID: {{ game.id }} · {{ game.executable_count }} exe
              </div>
            </div>
          </div>

          <div class="mt-5 flex flex-col gap-2 sm:flex-row sm:justify-end">
            <button
              type="button"
              class="rounded-md border border-gray-200 px-4 py-2 text-sm font-semibold text-gray-700 hover:bg-gray-100 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800"
              @click="closeNewGamesModal"
            >
              {{ appState.t('newGames.close') }}
            </button>
            <button
              type="button"
              class="rounded-md bg-indigo-600 px-4 py-2 text-sm font-semibold text-white hover:bg-indigo-500"
              @click="openNewGamesInSettings"
            >
              {{ appState.t('newGames.openSettings') }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </MainLayout>
</template>
