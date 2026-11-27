<script setup lang="ts">
import WallpaperSettings from '@/components/WallpaperSettings.vue';
import { computed, onMounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { confirm } from '@tauri-apps/plugin-dialog';
import {
    APP_THEME_OPTIONS,
    CLOSE_GAME_BEHAVIORS,
    SEARCH_PRIORITY_TARGET_MAX,
    SEARCH_PRIORITY_TARGET_MIN,
    useGlobalState,
} from '@/composables/state';
import type { AppLanguage, AppTheme, CloseGameBehavior } from '@/composables/state';
import { useSearchPriority } from '@/composables/search-priority';
import { useNewGames } from '@/composables/new-games';

interface CleanupResult {
    deleted_files: number;
    deleted_dirs: number;
    bytes_freed: number;
}

const {
    language,
    setLanguage,
    theme,
    setTheme,
    closeGameBehavior,
    setCloseGameBehavior,
    searchPriorityTargetCount,
    setSearchPriorityTargetCount,
    t,
} = useGlobalState();
const {
    priorityData,
    refreshSearchPriority,
    isLoading: isRefreshingPriority,
    refreshed: isPriorityRefreshed,
    usedCache: isPriorityUsingCache,
    errorMessage: priorityErrorMessage,
} = useSearchPriority();
const {
    state: newGamesState,
    errorMessage: newGamesErrorMessage,
    loadNewGamesState,
    clearNewGames,
} = useNewGames();

const isClearingIconCache = ref(false);
const isClearingGeneratedGames = ref(false);
const cleanupMessage = ref('');
const priorityMessage = ref('');

const selectedLanguage = computed({
    get: () => language.value,
    set: (value: AppLanguage) => setLanguage(value),
});

const selectedCloseGameBehavior = computed({
    get: () => closeGameBehavior.value,
    set: (value: CloseGameBehavior) => setCloseGameBehavior(value),
});

const selectedTheme = computed({
    get: () => theme.value,
    set: (value: AppTheme) => setTheme(value),
});

const selectedSearchPriorityTargetCount = computed({
    get: () => searchPriorityTargetCount.value,
    set: (value: number | string) => {
        setSearchPriorityTargetCount(Number(value));
        priorityMessage.value = t('settings.searchPrioritySaved');
    },
});

const priorityLoadedText = computed(() => {
    return t('settings.searchPriorityCurrent').replace('{count}', String(priorityData.value.items.length));
});

const visibleNewGames = computed(() => newGamesState.value.new_games.slice(0, 24));

const themeSwatches: Record<AppTheme, string> = {
    current: 'linear-gradient(135deg, #111827 0%, #1f2937 52%, #4f46e5 100%)',
    dark: 'linear-gradient(135deg, #05070b 0%, #111827 58%, #7c3aed 100%)',
    light: 'linear-gradient(135deg, #ffffff 0%, #eef1f6 58%, #4f46e5 100%)',
    purple: 'linear-gradient(135deg, #160f27 0%, #2d214b 58%, #a78bfa 100%)',
    blue: 'linear-gradient(135deg, #071525 0%, #12304f 58%, #38bdf8 100%)',
    green: 'linear-gradient(135deg, #061b15 0%, #123d31 58%, #34d399 100%)',
    red: 'linear-gradient(135deg, #230d13 0%, #3d1824 58%, #fb7185 100%)',
};

function getThemeLabel(themeOption: AppTheme) {
    return t(`settings.theme${themeOption.charAt(0).toUpperCase()}${themeOption.slice(1)}`);
}

function formatBytes(bytes: number) {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
    return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

function formatCleanupResult(prefix: string, result: CleanupResult) {
    return `${prefix} ${result.deleted_files} ${t('settings.deletedFiles')}, ${result.deleted_dirs} ${t('settings.deletedDirs')}, ${formatBytes(result.bytes_freed)} ${t('settings.freed')}.`;
}

function formatMetadataTime(seconds: number) {
    if (!seconds) return t('settings.newGamesNeverChecked');
    return new Date(seconds * 1000).toLocaleString();
}

async function clearIconCache() {
    const accepted = await confirm(t('settings.confirmIconCache'), {
        title: t('settings.cacheTitle'),
        kind: 'warning',
    });
    if (!accepted) return;

    isClearingIconCache.value = true;
    cleanupMessage.value = '';
    try {
        const result = await invoke<CleanupResult>('clear_icon_cache');
        cleanupMessage.value = formatCleanupResult(t('settings.clearedIconCache'), result);
    } catch (error) {
        cleanupMessage.value = `${t('settings.failed')} ${String(error)}`;
    } finally {
        isClearingIconCache.value = false;
    }
}

async function clearGeneratedGames() {
    const accepted = await confirm(t('settings.confirmGeneratedGames'), {
        title: t('settings.cacheTitle'),
        kind: 'warning',
    });
    if (!accepted) return;

    isClearingGeneratedGames.value = true;
    cleanupMessage.value = '';
    try {
        const result = await invoke<CleanupResult>('clear_generated_games');
        cleanupMessage.value = formatCleanupResult(t('settings.clearedGeneratedGames'), result);
    } catch (error) {
        cleanupMessage.value = `${t('settings.failed')} ${String(error)}`;
    } finally {
        isClearingGeneratedGames.value = false;
    }
}

async function refreshPriorityFromSettings() {
    priorityMessage.value = '';
    await refreshSearchPriority(selectedSearchPriorityTargetCount.value);

    if (isPriorityUsingCache.value && !isPriorityRefreshed.value) {
        priorityMessage.value = t('home.priorityCached');
    } else if (priorityErrorMessage.value) {
        priorityMessage.value = `${t('settings.searchPriorityFailed')} ${priorityErrorMessage.value}`;
    } else {
        priorityMessage.value = t('settings.searchPriorityUpdated');
    }
}

onMounted(() => {
    void loadNewGamesState();
});
</script>

<template>
    <div class="app-page app-page-wide">
        <h1 class="text-3xl font-bold text-gray-900 dark:text-white mb-6">
            {{ t('settings.title') }}
        </h1>

        <div class="grid grid-cols-1 gap-5 lg:grid-cols-2">
            <div class="space-y-5">
                <section class="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-5">
                    <label for="close-game-behavior" class="block text-lg font-semibold text-gray-900 dark:text-white">
                        {{ t('settings.closeGameBehavior') }}
                    </label>
                    <p id="close-game-description" class="mt-1 text-sm text-gray-500 dark:text-gray-400">
                        {{ t('settings.closeGameDescription') }}
                    </p>
                    <select
                        id="close-game-behavior"
                        v-model="selectedCloseGameBehavior"
                        aria-describedby="close-game-description"
                        class="mt-3 w-full rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm dark:border-gray-600 dark:bg-gray-700 dark:text-white"
                    >
                        <option v-for="behavior in CLOSE_GAME_BEHAVIORS" :key="behavior" :value="behavior">
                            {{ t('closeGames.' + behavior) }}
                        </option>
                    </select>
                </section>
                <section class="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-5">
                    <h2 class="text-lg font-semibold text-gray-900 dark:text-white mb-4">
                        {{ t('settings.language') }}
                    </h2>
                    <label class="block text-sm text-gray-500 dark:text-gray-400 mb-2" for="language-select">
                        {{ t('settings.languageLabel') }}
                    </label>
                    <select
                        id="language-select"
                        v-model="selectedLanguage"
                        class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-700 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500"
                    >
                        <option value="en">English</option>
                        <option value="tr">Türkçe</option>
                    </select>
                </section>

                <section class="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-5">
                    <h2 class="text-lg font-semibold text-gray-900 dark:text-white mb-2">
                        {{ t('settings.searchPriorityTitle') }}
                    </h2>
                    <p class="text-sm text-gray-500 dark:text-gray-400">
                        {{ t('settings.searchPriorityDescription') }}
                    </p>

                    <label class="mt-4 block text-sm text-gray-500 dark:text-gray-400" for="priority-target-input">
                        {{ t('settings.searchPriorityTarget') }}
                    </label>
                    <div class="mt-2 flex items-center gap-2">
                        <input
                            id="priority-target-input"
                            v-model.number="selectedSearchPriorityTargetCount"
                            type="number"
                            :min="SEARCH_PRIORITY_TARGET_MIN"
                            :max="SEARCH_PRIORITY_TARGET_MAX"
                            step="100"
                            class="min-w-0 flex-1 rounded-md border border-gray-300 bg-white px-3 py-2 text-gray-900 focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 dark:border-gray-600 dark:bg-gray-700 dark:text-white"
                        >
                        <button
                            type="button"
                            class="shrink-0 rounded-md bg-gray-700 px-3 py-2 text-sm font-semibold text-white hover:bg-gray-600 disabled:cursor-wait disabled:bg-gray-500"
                            :disabled="isRefreshingPriority"
                            @click="refreshPriorityFromSettings"
                        >
                            {{ isRefreshingPriority ? t('settings.searchPriorityRefreshing') : t('settings.searchPriorityRefresh') }}
                        </button>
                    </div>
                    <div class="mt-2 text-xs text-gray-500 dark:text-gray-400">
                        {{ SEARCH_PRIORITY_TARGET_MIN }}-{{ SEARCH_PRIORITY_TARGET_MAX }} · {{ priorityLoadedText }}
                    </div>
                    <div v-if="priorityMessage" class="mt-3 text-sm text-gray-500 dark:text-gray-400">
                        {{ priorityMessage }}
                    </div>
                </section>

                <section class="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-5">
                    <h2 class="text-lg font-semibold text-gray-900 dark:text-white">
                        {{ t('settings.themeTitle') }}
                    </h2>
                    <p class="mt-2 text-sm text-gray-500 dark:text-gray-400">
                        {{ t('settings.themeLabel') }}
                    </p>

                    <div class="mt-5 grid grid-cols-2 gap-3 sm:grid-cols-3 xl:grid-cols-4">
                        <button
                            v-for="themeOption in APP_THEME_OPTIONS"
                            :key="themeOption"
                            type="button"
                            class="group rounded-md border p-2 text-left transition-colors"
                            :class="selectedTheme === themeOption
                                ? 'border-indigo-500 bg-indigo-50 dark:bg-indigo-500/10'
                                : 'border-gray-200 bg-gray-50 hover:bg-gray-100 dark:border-gray-700 dark:bg-gray-900 dark:hover:bg-gray-700'"
                            @click="selectedTheme = themeOption"
                        >
                            <span
                                class="block h-10 rounded-md border border-black/10"
                                :style="{ background: themeSwatches[themeOption] }"
                            ></span>
                            <span class="mt-2 block text-sm font-medium text-gray-800 dark:text-white">
                                {{ getThemeLabel(themeOption) }}
                            </span>
                        </button>
                    </div>
                </section>
            </div>

            <div class="space-y-5">
                <section class="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-5">
                <h2 class="text-lg font-semibold text-gray-900 dark:text-white mb-5">
                    {{ t('settings.cacheTitle') }}
                </h2>

                <div class="divide-y divide-gray-200 dark:divide-gray-700">
                    <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3 py-4 first:pt-0">
                        <div>
                            <div class="font-medium text-gray-800 dark:text-white">
                                {{ t('settings.iconCache') }}
                            </div>
                        </div>
                        <button
                            type="button"
                            class="rounded-md bg-gray-700 hover:bg-gray-600 disabled:bg-gray-500 disabled:cursor-wait text-white px-4 py-2 text-sm"
                            :disabled="isClearingIconCache"
                            @click="clearIconCache"
                        >
                            {{ t('settings.clearIconCache') }}
                        </button>
                    </div>

                    <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3 py-4">
                        <div>
                            <div class="font-medium text-gray-800 dark:text-white">
                                {{ t('settings.generatedGames') }}
                            </div>
                        </div>
                        <button
                            type="button"
                            class="rounded-md bg-red-600 hover:bg-red-500 disabled:bg-red-400 disabled:cursor-wait text-white px-4 py-2 text-sm"
                            :disabled="isClearingGeneratedGames"
                            @click="clearGeneratedGames"
                        >
                            {{ t('settings.clearGeneratedGames') }}
                        </button>
                    </div>
                </div>

                <div class="mt-5 text-sm text-gray-500 dark:text-gray-400">
                    {{ cleanupMessage || t('settings.idle') }}
                </div>
                </section>

                <section
                    id="settings-new-games"
                    class="scroll-mt-6 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg p-5"
                >
                    <div class="flex items-start justify-between gap-3">
                        <div>
                            <h2 class="text-lg font-semibold text-gray-900 dark:text-white">
                                {{ t('settings.newGamesTitle') }}
                            </h2>
                            <div class="mt-1 text-sm text-gray-500 dark:text-gray-400">
                                {{ t('settings.newGamesDescription').replace('{count}', String(newGamesState.new_games.length)) }}
                            </div>
                        </div>
                        <button
                            type="button"
                            class="shrink-0 rounded-md bg-gray-700 px-3 py-1.5 text-xs font-semibold text-white hover:bg-gray-600"
                            @click="loadNewGamesState"
                        >
                            {{ t('generated.refresh') }}
                        </button>
                    </div>

                    <div class="mt-3 grid grid-cols-1 gap-2 text-xs text-gray-500 dark:text-gray-400 sm:grid-cols-2">
                        <div>
                            {{ t('settings.newGamesKnownCount').replace('{count}', String(newGamesState.known_count)) }}
                        </div>
                        <div>
                            {{ t('settings.newGamesLastChecked').replace('{time}', formatMetadataTime(newGamesState.last_checked_at)) }}
                        </div>
                    </div>

                    <div v-if="newGamesErrorMessage" class="mt-3 text-sm text-red-500 dark:text-red-300">
                        {{ newGamesErrorMessage }}
                    </div>

                    <div v-if="visibleNewGames.length > 0" class="mt-4 max-h-72 space-y-2 overflow-y-auto pr-1">
                        <div
                            v-for="game in visibleNewGames"
                            :key="game.id"
                            class="rounded-md border border-gray-200 bg-gray-50 px-3 py-2 dark:border-gray-700 dark:bg-gray-900"
                        >
                            <div class="flex items-start justify-between gap-3">
                                <div class="min-w-0">
                                    <div class="truncate font-semibold text-gray-900 dark:text-white" :title="game.name">
                                        {{ game.name }}
                                    </div>
                                    <div class="mt-1 text-xs text-gray-500 dark:text-gray-400">
                                        ID: {{ game.id }} · {{ game.executable_count }} exe
                                    </div>
                                </div>
                                <div class="shrink-0 text-right text-[11px] text-gray-400">
                                    {{ formatMetadataTime(game.detected_at) }}
                                </div>
                            </div>
                        </div>
                    </div>

                    <div v-else class="mt-4 rounded-md border border-gray-200 bg-gray-50 px-3 py-2 text-sm text-gray-500 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-400">
                        {{ t('settings.newGamesEmpty') }}
                    </div>

                    <button
                        v-if="newGamesState.new_games.length > 0"
                        type="button"
                        class="mt-4 w-full rounded-md border border-gray-200 px-4 py-2 text-sm font-semibold text-gray-700 hover:bg-gray-100 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-900"
                        @click="clearNewGames"
                    >
                        {{ t('settings.newGamesClear') }}
                    </button>
                </section>
                <WallpaperSettings />
            </div>
        </div>
    </div>
</template>
