<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { confirm } from '@tauri-apps/plugin-dialog';
import { useGlobalState } from '@/composables/state';
import { useSearchPriority } from '@/composables/search-priority';
import { buildPaginationItems, type PaginationItem } from '@/utils/pagination';
import { rankGeneratedGames, type GameSortMode } from '@/utils/search-ranking';

interface GeneratedExecutable {
    filename: string;
    relative_path: string;
    full_path: string;
    size: number;
    modified: number;
}

interface GeneratedGameSummary {
    app_id: string;
    name: string | null;
    folder_path: string;
    modified: number;
}

interface GeneratedGame extends GeneratedGameSummary {
    exe_count: number;
    size: number;
    executables: GeneratedExecutable[];
}

type ViewMode = 'compact' | 'detailed';

const { t } = useGlobalState();
const { priorityLookup } = useSearchPriority();

const PAGE_SIZE = 150;

const games = ref<GeneratedGameSummary[]>([]);
const detailsById = ref<Record<string, GeneratedGame>>({});
const loadingDetailsById = ref<Record<string, boolean>>({});
const searchQuery = ref('');
const statusMessage = ref('');
const isLoading = ref(false);
const deletingGameId = ref<string | null>(null);
const viewMode = ref<ViewMode>('compact');
const sortMode = ref<GameSortMode>('az');
const currentPage = ref(1);
const activeJumpKey = ref<string | null>(null);
const jumpPageInput = ref('');
const jumpInputRef = ref<HTMLInputElement | null>(null);

const rankedGames = computed(() => {
    return rankGeneratedGames(
        games.value,
        searchQuery.value,
        sortMode.value,
        priorityLookup.value,
        (game) => detailsById.value[game.app_id]?.executables.map(executable => executable.relative_path) ?? [],
    );
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

function getGameName(game: GeneratedGameSummary) {
    return game.name || game.app_id;
}

function getDisplayDetails(game: GeneratedGameSummary) {
    return detailsById.value[game.app_id] ?? null;
}

function isDetailsLoading(game: GeneratedGameSummary) {
    return Boolean(loadingDetailsById.value[game.app_id]);
}

function formatBytes(bytes: number) {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
    return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

function formatDate(seconds: number) {
    if (!seconds) return '-';
    return new Date(seconds * 1000).toLocaleString();
}

function setViewMode(newViewMode: ViewMode) {
    viewMode.value = newViewMode;
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

async function loadGeneratedGames() {
    isLoading.value = true;
    statusMessage.value = '';
    currentPage.value = 1;
    closeJumpInput();

    try {
        games.value = await invoke<GeneratedGameSummary[]>('list_generated_game_summaries');
        detailsById.value = {};
        loadingDetailsById.value = {};
    } catch (error) {
        statusMessage.value = `${t('generated.loadFailed')} ${String(error)}`;
    } finally {
        isLoading.value = false;
    }
}

async function loadGameDetails(game: GeneratedGameSummary) {
    if (detailsById.value[game.app_id] || loadingDetailsById.value[game.app_id]) return;

    loadingDetailsById.value = {
        ...loadingDetailsById.value,
        [game.app_id]: true,
    };

    try {
        const details = await invoke<GeneratedGame>('get_generated_game_details', { app_id: game.app_id });
        detailsById.value = {
            ...detailsById.value,
            [game.app_id]: details,
        };
    } catch (error) {
        statusMessage.value = `${t('generated.loadFailed')} ${String(error)}`;
    } finally {
        const { [game.app_id]: _removed, ...rest } = loadingDetailsById.value;
        loadingDetailsById.value = rest;
    }
}

async function openGameFolder(game: GeneratedGameSummary) {
    try {
        await invoke('open_generated_game_folder', { app_id: game.app_id });
    } catch (error) {
        statusMessage.value = String(error);
    }
}

async function deleteGame(game: GeneratedGameSummary) {
    const accepted = await confirm(
        formatMessage(t('generated.confirmDelete'), { name: getGameName(game) }),
        { title: t('generated.delete'), kind: 'warning' },
    );
    if (!accepted) return;

    deletingGameId.value = game.app_id;
    statusMessage.value = '';

    try {
        await invoke('delete_generated_game', { app_id: game.app_id });
        games.value = games.value.filter(item => item.app_id !== game.app_id);

        const { [game.app_id]: _details, ...detailsRest } = detailsById.value;
        const { [game.app_id]: _loading, ...loadingRest } = loadingDetailsById.value;
        detailsById.value = detailsRest;
        loadingDetailsById.value = loadingRest;

        statusMessage.value = t('generated.deleted');
    } catch (error) {
        statusMessage.value = `${t('generated.deleteFailed')} ${String(error)}`;
    } finally {
        deletingGameId.value = null;
    }
}

watch([searchQuery, viewMode, sortMode], () => {
    currentPage.value = 1;
    closeJumpInput();
});

watch(totalPages, (newTotalPages) => {
    if (currentPage.value > newTotalPages) {
        currentPage.value = newTotalPages;
    }
});

onMounted(() => {
    loadGeneratedGames();
});
</script>

<template>
    <div class="app-page app-page-wide">
        <div class="mb-6 flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
            <div>
                <h1 class="text-3xl font-bold text-gray-900 dark:text-white">
                    {{ t('generated.title') }}
                </h1>
                <div class="mt-2 text-sm text-gray-500 dark:text-gray-400">
                    {{ statusMessage || formatMessage(t('generated.resultCount'), { count: rankedGames.length }) }}
                </div>
            </div>

            <div class="flex items-center gap-2">
                <select
                    v-model="sortMode"
                    class="rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-800 focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 dark:border-gray-700 dark:bg-gray-800 dark:text-white"
                    :aria-label="t('generated.sortLabel')"
                    @change="setSortMode(sortMode)"
                >
                    <option value="az">{{ t('common.sortAz') }}</option>
                    <option value="popular">{{ t('common.sortPopular') }}</option>
                    <option value="id">{{ t('common.sortId') }}</option>
                </select>

                <div class="flex rounded-md border border-gray-200 bg-white p-1 dark:border-gray-700 dark:bg-gray-800">
                    <button
                        type="button"
                        class="rounded p-2 transition-colors"
                        :class="viewMode === 'compact'
                            ? 'bg-indigo-600 text-white'
                            : 'text-gray-500 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-700'"
                        :title="t('generated.compactView')"
                        :aria-label="t('generated.compactView')"
                        @click="setViewMode('compact')"
                    >
                        <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" aria-hidden="true">
                            <circle cx="8" cy="8" r="3" stroke="currentColor" stroke-width="2" />
                            <circle cx="16" cy="8" r="3" stroke="currentColor" stroke-width="2" />
                            <path d="M3.5 19c.7-3 2.2-5 4.5-5s3.8 2 4.5 5" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
                            <path d="M11.5 19c.7-3 2.2-5 4.5-5s3.8 2 4.5 5" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
                        </svg>
                    </button>
                    <button
                        type="button"
                        class="rounded p-2 transition-colors"
                        :class="viewMode === 'detailed'
                            ? 'bg-indigo-600 text-white'
                            : 'text-gray-500 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-700'"
                        :title="t('generated.detailedView')"
                        :aria-label="t('generated.detailedView')"
                        @click="setViewMode('detailed')"
                    >
                        <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" aria-hidden="true">
                            <rect x="4" y="5" width="16" height="4" rx="1" stroke="currentColor" stroke-width="2" />
                            <rect x="4" y="15" width="16" height="4" rx="1" stroke="currentColor" stroke-width="2" />
                            <path d="M8 9v6M14 9v6" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
                        </svg>
                    </button>
                </div>

                <button
                    type="button"
                    class="rounded-md bg-gray-700 px-4 py-2 text-sm font-semibold text-white hover:bg-gray-600 disabled:cursor-wait disabled:bg-gray-500"
                    :disabled="isLoading"
                    @click="loadGeneratedGames"
                >
                    {{ t('generated.refresh') }}
                </button>
            </div>
        </div>

        <div class="mb-5">
            <input
                v-model="searchQuery"
                type="text"
                :placeholder="t('generated.searchPlaceholder')"
                class="w-full rounded-lg border border-gray-300 px-4 py-2 text-gray-900 focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500 dark:border-gray-600 dark:bg-gray-700 dark:text-white"
            />
        </div>

        <section class="overflow-hidden rounded-lg border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-800">
            <div v-if="isLoading" class="px-5 py-10 text-center text-gray-500 dark:text-gray-400">
                {{ t('generated.detailsLoading') }}
            </div>

            <div v-else-if="rankedGames.length === 0" class="px-5 py-10 text-center text-gray-500 dark:text-gray-400">
                {{ t('generated.empty') }}
            </div>

            <div v-else>
                <div v-if="viewMode === 'compact'" class="divide-y divide-gray-200 dark:divide-gray-700">
                    <div
                        v-for="game in pageGames"
                        :key="game.app_id"
                        class="grid grid-cols-1 gap-3 px-4 py-3 md:grid-cols-[1fr_auto] md:items-center"
                    >
                        <div class="min-w-0">
                            <div class="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1">
                                <div class="truncate text-sm font-semibold text-gray-900 dark:text-white">
                                    {{ getGameName(game) }}
                                </div>
                                <div class="select-all font-mono text-xs text-gray-500 dark:text-gray-400">
                                    {{ game.app_id }}
                                </div>
                            </div>
                            <div class="mt-1 flex flex-wrap gap-x-4 gap-y-1 text-xs text-gray-500 dark:text-gray-400">
                                <span>{{ formatDate(game.modified) }}</span>
                                <span v-if="getDisplayDetails(game)">
                                    {{ getDisplayDetails(game)?.exe_count }} {{ t('generated.executables') }}
                                </span>
                                <span v-if="getDisplayDetails(game)">
                                    {{ formatBytes(getDisplayDetails(game)?.size ?? 0) }}
                                </span>
                            </div>
                        </div>

                        <div class="flex flex-wrap gap-2 md:justify-end">
                            <button
                                v-if="!getDisplayDetails(game)"
                                type="button"
                                class="rounded-md bg-gray-200 px-3 py-1.5 text-xs font-semibold text-gray-700 hover:bg-gray-300 disabled:cursor-wait dark:bg-gray-700 dark:text-gray-200 dark:hover:bg-gray-600"
                                :disabled="isDetailsLoading(game)"
                                @click="loadGameDetails(game)"
                            >
                                {{ isDetailsLoading(game) ? t('generated.detailsLoading') : t('generated.loadDetails') }}
                            </button>
                            <button
                                type="button"
                                class="rounded-md bg-gray-700 px-3 py-1.5 text-xs font-semibold text-white hover:bg-gray-600"
                                @click="openGameFolder(game)"
                            >
                                {{ t('generated.openFolder') }}
                            </button>
                            <button
                                type="button"
                                class="rounded-md bg-red-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-red-500 disabled:cursor-wait disabled:bg-red-400"
                                :disabled="deletingGameId === game.app_id"
                                @click="deleteGame(game)"
                            >
                                {{ t('generated.delete') }}
                            </button>
                        </div>
                    </div>
                </div>

                <div v-else class="divide-y divide-gray-200 dark:divide-gray-700">
                    <div
                        v-for="game in pageGames"
                        :key="game.app_id"
                        class="grid grid-cols-1 gap-4 px-5 py-4 xl:grid-cols-[minmax(220px,1fr)_minmax(180px,0.75fr)_minmax(220px,1fr)_auto] xl:items-center"
                    >
                        <div class="min-w-0">
                            <div class="truncate text-base font-semibold text-gray-900 dark:text-white">
                                {{ getGameName(game) }}
                            </div>
                            <div class="mt-1 select-all break-all rounded-md bg-gray-100 px-2 py-1 font-mono text-xs text-gray-700 dark:bg-gray-900 dark:text-gray-300">
                                {{ game.app_id }}
                            </div>
                        </div>

                        <div v-if="getDisplayDetails(game)" class="grid grid-cols-3 gap-3 text-sm">
                            <div>
                                <div class="text-xs uppercase text-gray-400">{{ t('generated.executables') }}</div>
                                <div class="font-semibold text-gray-800 dark:text-white">{{ getDisplayDetails(game)?.exe_count }}</div>
                            </div>
                            <div>
                                <div class="text-xs uppercase text-gray-400">{{ t('generated.size') }}</div>
                                <div class="font-semibold text-gray-800 dark:text-white">{{ formatBytes(getDisplayDetails(game)?.size ?? 0) }}</div>
                            </div>
                            <div>
                                <div class="text-xs uppercase text-gray-400">{{ t('generated.modified') }}</div>
                                <div class="font-semibold text-gray-800 dark:text-white">{{ formatDate(getDisplayDetails(game)?.modified ?? game.modified) }}</div>
                            </div>
                        </div>
                        <div v-else class="text-sm text-gray-500 dark:text-gray-400">
                            {{ isDetailsLoading(game) ? t('generated.detailsLoading') : t('generated.detailsUnavailable') }}
                        </div>

                        <div class="min-w-0">
                            <template v-if="getDisplayDetails(game)">
                                <div class="mb-1 text-xs font-medium uppercase text-gray-400">
                                    {{ t('generated.executables') }}
                                </div>
                                <div class="flex flex-wrap gap-1">
                                    <span
                                        v-for="executable in getDisplayDetails(game)?.executables.slice(0, 3) ?? []"
                                        :key="executable.full_path"
                                        class="max-w-full truncate rounded-md border border-gray-200 px-2 py-1 font-mono text-xs text-gray-500 dark:border-gray-700 dark:text-gray-400"
                                    >
                                        {{ executable.relative_path }}
                                    </span>
                                    <span
                                        v-if="(getDisplayDetails(game)?.executables.length ?? 0) > 3"
                                        class="rounded-md border border-gray-200 px-2 py-1 text-xs text-gray-500 dark:border-gray-700 dark:text-gray-400"
                                    >
                                        +{{ (getDisplayDetails(game)?.executables.length ?? 0) - 3 }}
                                    </span>
                                </div>
                            </template>
                            <button
                                v-else
                                type="button"
                                class="rounded-md bg-gray-200 px-3 py-2 text-sm font-semibold text-gray-700 hover:bg-gray-300 disabled:cursor-wait dark:bg-gray-700 dark:text-gray-200 dark:hover:bg-gray-600"
                                :disabled="isDetailsLoading(game)"
                                @click="loadGameDetails(game)"
                            >
                                {{ isDetailsLoading(game) ? t('generated.detailsLoading') : t('generated.loadDetails') }}
                            </button>
                        </div>

                        <div class="flex flex-wrap gap-2 xl:justify-end">
                            <button
                                type="button"
                                class="rounded-md bg-gray-700 px-3 py-2 text-sm font-semibold text-white hover:bg-gray-600"
                                @click="openGameFolder(game)"
                            >
                                {{ t('generated.openFolder') }}
                            </button>
                            <button
                                type="button"
                                class="rounded-md bg-red-600 px-3 py-2 text-sm font-semibold text-white hover:bg-red-500 disabled:cursor-wait disabled:bg-red-400"
                                :disabled="deletingGameId === game.app_id"
                                @click="deleteGame(game)"
                            >
                                {{ t('generated.delete') }}
                            </button>
                        </div>
                    </div>
                </div>

                <div class="flex flex-col gap-3 border-t border-gray-200 px-5 py-4 dark:border-gray-700 lg:flex-row lg:items-center lg:justify-between">
                    <div class="text-sm text-gray-500 dark:text-gray-400">
                        {{ formatMessage(t('generated.pageSummary'), {
                            start: pageStart,
                            end: pageEnd,
                            total: rankedGames.length,
                            current: currentPage,
                            pages: totalPages,
                        }) }}
                    </div>

                    <nav class="flex flex-wrap items-center gap-1" :aria-label="t('generated.pagination')">
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
