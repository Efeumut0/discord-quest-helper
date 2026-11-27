<script setup lang="ts">
import { computed, nextTick, onActivated, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useFetchGameList } from '@/composables/fetch-gamelist';
import { useGlobalState } from '@/composables/state';
import { useSearchPriority } from '@/composables/search-priority';
import type { Game } from '@/types/types';
import { buildPaginationItems, type PaginationItem } from '@/utils/pagination';
import {
    getExecutableFilename,
    getExecutableLaunchPayload,
    getPlayableExecutables,
    getSupportedExecutables,
} from '@/utils/game-executables';
import { rankGames, type GameSortMode } from '@/utils/search-ranking';

const {
    gameDB,
    fetchGameList,
    isLoadingBundled,
    isLoadingDiscord,
} = useFetchGameList();

const { addLog, t } = useGlobalState();
const { priorityLookup } = useSearchPriority();

const PAGE_SIZE = 200;
const searchQuery = ref('');
const sortMode = ref<GameSortMode>('az');
const creatingGameId = ref<string | null>(null);
const statusMessage = ref('');
const generatedGameIds = ref(new Set<string>());

async function refreshGeneratedGames() {
    try {
        const games = await invoke<{ app_id: string; exe_count: number }[]>('list_generated_games');
        generatedGameIds.value = new Set(games.filter(game => game.exe_count > 0).map(game => game.app_id));
    } catch (error) {
        addLog('error', String(error));
    }
}

onActivated(refreshGeneratedGames);
watch(creatingGameId, (id, previousId) => {
    if (id === null && previousId !== null) void refreshGeneratedGames();
});
const currentPage = ref(1);
const activeJumpKey = ref<string | null>(null);
const jumpPageInput = ref('');
const jumpInputRef = ref<HTMLInputElement | null>(null);

const isLoading = computed(() => isLoadingBundled.value || isLoadingDiscord.value);

const rankedGames = computed(() => {
    return rankGames(gameDB.value, searchQuery.value, sortMode.value, priorityLookup.value);
});

const totalPages = computed(() => Math.max(1, Math.ceil(rankedGames.value.length / PAGE_SIZE)));
const pageStart = computed(() => {
    if (rankedGames.value.length === 0) return 0;
    return ((currentPage.value - 1) * PAGE_SIZE) + 1;
});
const pageEnd = computed(() => Math.min(currentPage.value * PAGE_SIZE, rankedGames.value.length));
const pageGames = computed(() => {
    const start = (currentPage.value - 1) * PAGE_SIZE;
    return rankedGames.value.slice(start, start + PAGE_SIZE);
});
const paginationItems = computed<PaginationItem[]>(() => buildPaginationItems(currentPage.value, totalPages.value));

function formatMessage(template: string, values: Record<string, string | number>) {
    return Object.entries(values).reduce(
        (message, [key, value]) => message.replace(`{${key}}`, String(value)),
        template,
    );
}

function setSortMode(newSortMode: GameSortMode) {
    sortMode.value = newSortMode;
}

function closeJumpInput() {
    activeJumpKey.value = null;
    jumpPageInput.value = '';
}

function goToPage(page: number) {
    if (!Number.isFinite(page)) return;

    const normalizedPage = Math.min(Math.max(Math.trunc(page), 1), totalPages.value);
    currentPage.value = normalizedPage;
    closeJumpInput();
}

async function openJumpInput(key: string) {
    activeJumpKey.value = key;
    jumpPageInput.value = String(currentPage.value);
    await nextTick();
    jumpInputRef.value?.focus();
    jumpInputRef.value?.select();
}

function submitJumpPage() {
    goToPage(Number(jumpPageInput.value));
}

async function createGameFiles(game: Game) {
    const executables = getPlayableExecutables(game);
    if (executables.length === 0) {
        statusMessage.value = t('games.noWindows');
        return;
    }

    creatingGameId.value = game.id;
    statusMessage.value = '';

    try {
        let createdCount = 0;
        for (const executable of executables) {
            const payload = getExecutableLaunchPayload(executable);

            await invoke('create_fake_game', {
                name: game.name,
                path: payload.path,
                executable_name: payload.filename,
                app_id: game.id,
                icon_hash: game.icon_hash ?? '',
            });
            createdCount += 1;
        }

        statusMessage.value = formatMessage(t('games.statusCreated'), {
            name: game.name,
            count: createdCount,
        });
        addLog('info', statusMessage.value);
    } catch (error) {
        const message = `${t('games.statusFailed')} ${String(error)}`;
        statusMessage.value = message;
        addLog('error', message);
    } finally {
        creatingGameId.value = null;
    }
}

watch([searchQuery, sortMode], () => {
    currentPage.value = 1;
    closeJumpInput();
});

watch(totalPages, (newTotalPages) => {
    if (currentPage.value > newTotalPages) {
        currentPage.value = newTotalPages;
    }
});
</script>

<template>
    <div class="app-page app-page-wide">
        <div class="mb-6 flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
            <div>
                <h1 class="text-3xl font-bold text-gray-900 dark:text-white">
                    {{ t('games.title') }}
                </h1>
                <div class="mt-2 text-sm text-gray-500 dark:text-gray-400">
                    {{ statusMessage || t('games.statusReady') }}
                </div>
            </div>

            <div class="flex w-full flex-col gap-2 sm:flex-row md:w-auto">
                <select
                    v-model="sortMode"
                    class="rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-800 focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 dark:border-gray-700 dark:bg-gray-800 dark:text-white"
                    :aria-label="t('games.sortLabel')"
                    @change="setSortMode(sortMode)"
                >
                    <option value="az">{{ t('common.sortAz') }}</option>
                    <option value="popular">{{ t('common.sortPopular') }}</option>
                    <option value="id">{{ t('common.sortId') }}</option>
                </select>

                <button
                    type="button"
                    class="w-full rounded-md bg-gray-700 px-4 py-2 text-sm text-white hover:bg-gray-600 disabled:cursor-wait disabled:bg-gray-500 sm:w-auto"
                    :disabled="isLoading"
                    @click="fetchGameList()"
                >
                    {{ t('games.refetch') }}
                </button>
            </div>
        </div>

        <div class="mb-5">
            <input
                v-model="searchQuery"
                type="text"
                :placeholder="t('games.searchPlaceholder')"
                class="w-full rounded-lg border border-gray-300 px-4 py-2 text-gray-900 focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 dark:border-gray-600 dark:bg-gray-700 dark:text-white"
            />
            <div class="mt-2 text-xs text-gray-500 dark:text-gray-400">
                {{ formatMessage(t('games.resultCount'), { count: rankedGames.length }) }}
            </div>
        </div>

        <section class="overflow-hidden rounded-lg border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-800">
            <div v-if="rankedGames.length === 0" class="px-5 py-10 text-center text-gray-500 dark:text-gray-400">
                {{ t('games.noResults') }}
            </div>

            <div v-else>
                <div class="divide-y divide-gray-200 dark:divide-gray-700">
                    <div
                        v-for="game in pageGames"
                        :key="game.id"
                        class="grid grid-cols-1 gap-4 px-5 py-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,0.85fr)_minmax(0,1.2fr)_18rem] lg:items-center"
                    >
                        <div class="min-w-0">
                            <div class="truncate text-base font-semibold text-gray-900 dark:text-white">
                                {{ game.name }}
                            </div>
                            <div v-if="game.aliases?.length" class="mt-1 truncate text-xs text-gray-500 dark:text-gray-400">
                                {{ game.aliases.join(', ') }}
                            </div>
                        </div>

                        <div>
                            <div class="mb-1 text-xs font-medium uppercase text-gray-400">
                                {{ t('games.id') }}
                            </div>
                            <div class="select-all break-all rounded-md bg-gray-100 px-2 py-1 font-mono text-xs text-gray-700 dark:bg-gray-900 dark:text-gray-300">
                                {{ game.id }}
                            </div>
                        </div>

                        <div class="min-w-0">
                            <div class="mb-1 text-xs font-medium uppercase text-gray-400">
                                {{ t('games.executables') }}
                            </div>
                            <div v-if="getSupportedExecutables(game).length" class="flex flex-wrap gap-1">
                                <span
                                    v-for="executable in getSupportedExecutables(game).slice(0, 3)"
                                    :key="executable.name"
                                    class="max-w-full truncate rounded-md border border-gray-200 px-2 py-1 font-mono text-xs text-gray-500 dark:border-gray-700 dark:text-gray-400"
                                >
                                    {{ getExecutableFilename(executable) }}
                                </span>
                                <span
                                    v-if="getSupportedExecutables(game).length > 3"
                                    class="rounded-md border border-gray-200 px-2 py-1 text-xs text-gray-500 dark:border-gray-700 dark:text-gray-400"
                                >
                                    +{{ getSupportedExecutables(game).length - 3 }}
                                </span>
                            </div>
                            <div v-else class="text-xs text-gray-500 dark:text-gray-400" :title="t('sources.description2')">
                                {{ t('games.noWindowsShort') }}
                            </div>
                        </div>

                        <div class="flex flex-wrap items-center justify-end gap-2">
                            <span v-if="generatedGameIds.has(game.id)" class="text-xs font-medium text-gray-700 dark:text-gray-200">
                                {{ t('games.filesExist') }}
                            </span>
                            <button
                                type="button"
                                class="rounded-md bg-indigo-600 px-4 py-2 text-sm text-white hover:bg-indigo-500 disabled:cursor-not-allowed disabled:bg-indigo-400 lg:w-48"
                                :disabled="creatingGameId !== null || getPlayableExecutables(game).length === 0"
                                @click="createGameFiles(game)"
                            >
                                {{ t(creatingGameId === game.id ? 'games.creating' : generatedGameIds.has(game.id) ? 'games.recreate' : 'games.create') }}
                            </button>
                        </div>
                    </div>
                </div>

                <div class="flex flex-col gap-3 border-t border-gray-200 px-5 py-4 dark:border-gray-700 lg:flex-row lg:items-center lg:justify-between">
                    <div class="text-sm text-gray-500 dark:text-gray-400">
                        {{ formatMessage(t('games.pageSummary'), {
                            start: pageStart,
                            end: pageEnd,
                            total: rankedGames.length,
                            current: currentPage,
                            pages: totalPages,
                        }) }}
                    </div>

                    <nav class="flex flex-wrap items-center gap-1" :aria-label="t('games.pagination')">
                        <button
                            type="button"
                            class="min-h-9 rounded-md border border-gray-200 px-3 text-sm font-semibold text-gray-600 transition-colors hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-40 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-700"
                            :disabled="currentPage === 1"
                            :aria-label="t('generated.previousPage')"
                            @click="goToPage(currentPage - 1)"
                        >
                            ‹
                        </button>

                        <template v-for="item in paginationItems" :key="item.type === 'page' ? item.page : item.key">
                            <button
                                v-if="item.type === 'page'"
                                type="button"
                                class="min-h-9 min-w-9 rounded-md border px-3 text-sm font-semibold transition-colors"
                                :class="currentPage === item.page
                                    ? 'border-indigo-600 bg-indigo-600 text-white'
                                    : 'border-gray-200 text-gray-600 hover:bg-gray-100 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-700'"
                                :aria-current="currentPage === item.page ? 'page' : undefined"
                                @click="goToPage(item.page)"
                            >
                                {{ item.page }}
                            </button>

                            <form
                                v-else-if="activeJumpKey === item.key"
                                class="flex items-center gap-1"
                                @submit.prevent="submitJumpPage"
                            >
                                <input
                                    ref="jumpInputRef"
                                    v-model="jumpPageInput"
                                    type="number"
                                    min="1"
                                    :max="totalPages"
                                    class="h-9 w-20 rounded-md border border-indigo-500 bg-white px-2 text-center text-sm font-semibold text-gray-900 focus:ring-2 focus:ring-indigo-500 dark:bg-gray-900 dark:text-white"
                                    :aria-label="t('generated.jumpToPage')"
                                    @keydown.escape="closeJumpInput"
                                />
                                <button
                                    type="submit"
                                    class="h-9 rounded-md bg-indigo-600 px-3 text-xs font-semibold text-white hover:bg-indigo-500"
                                >
                                    {{ t('generated.go') }}
                                </button>
                            </form>

                            <button
                                v-else
                                type="button"
                                class="min-h-9 min-w-9 rounded-md border border-gray-200 px-3 text-sm font-semibold text-gray-500 transition-colors hover:bg-gray-100 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-700"
                                :aria-label="t('generated.jumpToPage')"
                                @click="openJumpInput(item.key)"
                            >
                                …
                            </button>
                        </template>

                        <button
                            type="button"
                            class="min-h-9 rounded-md border border-gray-200 px-3 text-sm font-semibold text-gray-600 transition-colors hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-40 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-700"
                            :disabled="currentPage === totalPages"
                            :aria-label="t('generated.nextPage')"
                            @click="goToPage(currentPage + 1)"
                        >
                            ›
                        </button>
                    </nav>
                </div>
            </div>
        </section>
    </div>
</template>
