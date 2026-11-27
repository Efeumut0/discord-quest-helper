<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { LoaderCircle } from '@lucide/vue';
import { useGlobalState } from '@/composables/state';
import { useFetchGameList } from '@/composables/fetch-gamelist';
import { friendlyError } from '@/composables/friendly-errors';
import { buildActiveQuestCatalog } from '@/utils/active-quest-catalog';
import { getExecutablePath, getExecutableFilename, getSupportedExecutables } from '@/utils/game-executables';
import type { Game } from '@/types/types';

interface ActiveQuestListItem {
    id: string;
    name: string;
    game_names: string[];
    expires_at: number;
    starts_at: number;
    reward: string | null;
    publisher: string | null;
    hero_image: string | null;
    hero_video: string | null;
    variant_count: number;
    matched_game_id: string | null;
    matched_game_name: string | null;
    matched_game_icon_hash: string | null;
    has_windows_executable: boolean;
}

interface ActiveQuestListResponse {
    quests: ActiveQuestListItem[];
    stream_only_count: number;
    sources: string[];
    source_errors: string[];
}

const { t, language } = useGlobalState();
const { gameDB } = useFetchGameList();

const quests = ref<ActiveQuestListItem[]>([]);
const streamOnlyCount = ref(0);
const sources = ref<string[]>([]);
const sourceErrors = ref<string[]>([]);
const errorText = ref('');
const isLoading = ref(false);
const hasLoaded = ref(false);
const loadedAt = ref<Date | null>(null);

const isCatalogReady = computed(() => gameDB.value.length > 0);

function iconUrl(quest: ActiveQuestListItem) {
    if (!quest.matched_game_id || !quest.matched_game_icon_hash) return null;
    return `https://cdn.discordapp.com/app-icons/${quest.matched_game_id}/${quest.matched_game_icon_hash}.png?size=128`;
}

function initials(quest: ActiveQuestListItem) {
    const source = quest.matched_game_name || quest.game_names[0] || quest.name;
    return source.trim().slice(0, 2).toUpperCase();
}

function gameLabel(quest: ActiveQuestListItem) {
    return quest.matched_game_name || quest.game_names[0] || t('quests.unknownGame');
}

function formatDate(seconds: number) {
    if (!seconds) return '';
    return new Date(seconds * 1000).toLocaleString(language.value === 'tr' ? 'tr-TR' : 'en-US', {
        day: '2-digit',
        month: '2-digit',
        year: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
    });
}

function remainingText(expiresAt: number) {
    const seconds = expiresAt - Math.floor(Date.now() / 1000);
    if (seconds <= 0) return t('quests.expired');
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    if (days > 0) return `${days}${t('quests.dayShort')} ${hours}${t('quests.hourShort')}`;
    const minutes = Math.floor((seconds % 3600) / 60);
    return `${hours}${t('quests.hourShort')} ${minutes}${t('quests.minuteShort')}`;
}

const brokenMedia = ref<string[]>([]);

function hasMedia(quest: ActiveQuestListItem) {
    return Boolean(quest.hero_video || quest.hero_image) && !brokenMedia.value.includes(quest.id);
}

function markMediaBroken(quest: ActiveQuestListItem) {
    if (!brokenMedia.value.includes(quest.id)) brokenMedia.value = [...brokenMedia.value, quest.id];
}

interface RunningGeneratedGame {
    app_id: string;
    path: string;
    executable_name: string;
}

const busyIds = ref<string[]>([]);
const runningGames = ref<RunningGeneratedGame[]>([]);
let runningTimer: ReturnType<typeof setInterval> | null = null;

async function refreshRunningGames() {
    try {
        runningGames.value = await invoke<RunningGeneratedGame[]>('get_running_generated_game_processes');
    } catch {
        // The panel keeps working without the process list; the next tick retries.
    }
}

function runningProcess(quest: ActiveQuestListItem) {
    if (!quest.matched_game_id) return null;
    return runningGames.value.find(item => item.app_id === quest.matched_game_id) ?? null;
}

function questGame(quest: ActiveQuestListItem): Game | null {
    if (!quest.matched_game_id) return null;
    return gameDB.value.find(game => game.id === quest.matched_game_id) ?? null;
}

function canCreate(quest: ActiveQuestListItem) {
    return quest.has_windows_executable && questGame(quest) !== null;
}

async function createGame(quest: ActiveQuestListItem) {
    const game = questGame(quest);
    if (!game || busyIds.value.includes(quest.id)) return;
    const executable = getSupportedExecutables(game)[0];
    if (!executable) return;

    busyIds.value = [...busyIds.value, quest.id];
    errorText.value = '';
    try {
        const path = executable.path ?? getExecutablePath(executable);
        const executableName = executable.filename ?? getExecutableFilename(executable);
        await invoke('create_fake_game', {
            name: game.name,
            path,
            executable_name: executableName,
            app_id: game.id,
            icon_hash: game.icon_hash ?? '',
        });
        await invoke('run_background_process', {
            name: game.name,
            path,
            executable_name: executableName,
            app_id: game.id,
        });
    } catch (error) {
        const raw = error instanceof Error ? error.message : String(error);
        errorText.value = friendlyError(raw, t);
    } finally {
        busyIds.value = busyIds.value.filter(id => id !== quest.id);
        await refreshRunningGames();
    }
}

async function stopGame(quest: ActiveQuestListItem) {
    const process = runningProcess(quest);
    if (!process || busyIds.value.includes(quest.id)) return;

    busyIds.value = [...busyIds.value, quest.id];
    errorText.value = '';
    try {
        await invoke('stop_process', {
            exec_name: process.executable_name,
            app_id: process.app_id,
            path: process.path,
        });
    } catch (error) {
        const raw = error instanceof Error ? error.message : String(error);
        errorText.value = friendlyError(raw, t);
    } finally {
        busyIds.value = busyIds.value.filter(id => id !== quest.id);
        await refreshRunningGames();
    }
}

function isUpcoming(quest: ActiveQuestListItem) {
    return quest.starts_at > Math.floor(Date.now() / 1000);
}

function variantText(quest: ActiveQuestListItem) {
    return t('quests.variants').replace('{count}', String(quest.variant_count - 1));
}

async function load() {
    if (isLoading.value || !isCatalogReady.value) return;
    isLoading.value = true;
    errorText.value = '';
    try {
        const response = await invoke<ActiveQuestListResponse>('list_active_quests', {
            games: buildActiveQuestCatalog(gameDB.value),
        });
        quests.value = response.quests;
        streamOnlyCount.value = response.stream_only_count;
        sources.value = response.sources;
        sourceErrors.value = response.source_errors;
        hasLoaded.value = true;
        loadedAt.value = new Date();
    } catch (error) {
        const raw = error instanceof Error ? error.message : String(error);
        errorText.value = friendlyError(raw, t);
    } finally {
        isLoading.value = false;
    }
}

onMounted(() => {
    if (isCatalogReady.value) void load();
    void refreshRunningGames();
    runningTimer = setInterval(() => void refreshRunningGames(), 3000);
});

onUnmounted(() => {
    if (runningTimer) clearInterval(runningTimer);
    runningTimer = null;
});

watch(isCatalogReady, ready => {
    if (ready && !hasLoaded.value) void load();
});
</script>

<template>
    <div class="app-page app-page-wide">
        <div class="mb-6 flex flex-wrap items-center justify-between gap-3">
            <div>
                <h1 class="text-3xl font-bold text-gray-900 dark:text-white">{{ t('quests.title') }}</h1>
                <p class="mt-1 text-sm text-gray-500 dark:text-gray-400">{{ t('quests.subtitle') }}</p>
            </div>
            <button
                type="button"
                class="inline-flex items-center gap-1.5 rounded-md bg-indigo-600 px-3 py-2 text-sm font-semibold text-white hover:bg-indigo-500 disabled:cursor-not-allowed disabled:opacity-50"
                :disabled="isLoading || !isCatalogReady"
                @click="load"
            >
                <LoaderCircle v-if="isLoading" class="h-4 w-4 animate-spin" />
                {{ isLoading ? t('quests.loading') : t('quests.refresh') }}
            </button>
        </div>

        <div v-if="!isCatalogReady" class="rounded-lg border border-gray-200 bg-white p-6 text-center text-sm text-gray-500 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-400">
            {{ t('quests.catalogLoading') }}
        </div>

        <div v-else-if="errorText" class="rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-900/20 dark:text-red-300">
            {{ errorText }}
        </div>

        <div v-else-if="hasLoaded && quests.length === 0" class="rounded-lg border border-gray-200 bg-white p-6 text-center text-sm text-gray-500 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-400">
            {{ t('quests.empty') }}
        </div>

        <div v-else class="grid grid-cols-2 gap-4 sm:grid-cols-3 xl:grid-cols-4">
            <article
                v-for="quest in quests"
                :key="quest.id"
                class="flex flex-col overflow-hidden rounded-xl border border-gray-200 bg-white shadow-sm dark:border-gray-700 dark:bg-gray-800"
            >
                <div class="relative aspect-[4/3] w-full shrink-0 bg-gray-100 dark:bg-gray-900">
                    <video
                        v-if="hasMedia(quest) && quest.hero_video"
                        :src="quest.hero_video"
                        :poster="quest.hero_image || undefined"
                        class="absolute inset-0 h-full w-full object-cover"
                        autoplay
                        loop
                        muted
                        playsinline
                        disablepictureinpicture
                        @error="markMediaBroken(quest)"
                    ></video>
                    <img
                        v-else-if="hasMedia(quest) && quest.hero_image"
                        :src="quest.hero_image"
                        alt=""
                        class="absolute inset-0 h-full w-full object-cover"
                        loading="lazy"
                        @error="markMediaBroken(quest)"
                    >
                    <div
                        v-else
                        class="absolute inset-0 flex items-center justify-center text-2xl font-bold text-gray-400 dark:text-gray-600"
                    >
                        {{ initials(quest) }}
                    </div>

                    <div class="absolute left-2 top-2 flex flex-wrap gap-1">
                        <span v-if="isUpcoming(quest)" class="rounded bg-sky-600/90 px-1.5 py-0.5 text-[10px] font-semibold text-white">
                            {{ t('quests.upcoming') }}
                        </span>
                        <span v-if="!quest.has_windows_executable" class="rounded bg-amber-600/90 px-1.5 py-0.5 text-[10px] font-semibold text-white">
                            {{ t('quests.noExecutable') }}
                        </span>
                        <span v-if="quest.variant_count > 1" class="rounded bg-gray-900/70 px-1.5 py-0.5 text-[10px] font-semibold text-white">
                            {{ variantText(quest) }}
                        </span>
                    </div>
                </div>

                <div class="flex flex-1 flex-col gap-3 p-4">
                    <div class="flex items-start gap-2.5">
                        <img
                            v-if="iconUrl(quest)"
                            :src="iconUrl(quest) || ''"
                            :alt="gameLabel(quest)"
                            class="h-9 w-9 shrink-0 rounded-md object-cover"
                            loading="lazy"
                        >
                        <div
                            v-else
                            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-gray-100 text-xs font-semibold text-gray-500 dark:bg-gray-700 dark:text-gray-300"
                        >
                            {{ initials(quest) }}
                        </div>
                        <div class="min-w-0">
                            <h2 class="truncate text-sm font-semibold leading-tight text-gray-900 dark:text-white">{{ gameLabel(quest) }}</h2>
                            <p v-if="quest.publisher" class="mt-0.5 truncate text-[11px] text-gray-500 dark:text-gray-400">{{ quest.publisher }}</p>
                        </div>
                    </div>

                    <p v-if="quest.reward" class="line-clamp-2 text-xs font-medium leading-snug text-gray-700 dark:text-gray-200">
                        {{ quest.reward }}
                    </p>

                    <div class="mt-auto flex items-end justify-between gap-2 border-t border-gray-100 pt-3 dark:border-gray-700/70">
                        <dl class="min-w-0 space-y-1.5 text-[11px] leading-relaxed text-gray-500 dark:text-gray-400">
                        <div>
                            <dt class="inline font-semibold">{{ t('quests.remaining') }}:</dt>
                            <dd class="ml-1 inline">{{ remainingText(quest.expires_at) }}</dd>
                        </div>
                        <div>
                            <dt class="inline font-semibold">{{ t('quests.ends') }}:</dt>
                            <dd class="ml-1 inline">{{ formatDate(quest.expires_at) }}</dd>
                        </div>
                        <div v-if="quest.starts_at && isUpcoming(quest)">
                            <dt class="inline font-semibold">{{ t('quests.starts') }}:</dt>
                            <dd class="ml-1 inline">{{ formatDate(quest.starts_at) }}</dd>
                        </div>
                        <div class="truncate">
                            <dt class="inline font-semibold">{{ t('quests.questId') }}:</dt>
                            <dd class="ml-1 inline font-mono text-[10px]">{{ quest.id }}</dd>
                        </div>
                        </dl>

                        <button
                            v-if="runningProcess(quest)"
                            type="button"
                            class="shrink-0 rounded-md bg-red-600 px-2.5 py-1 text-[11px] font-semibold text-white hover:bg-red-500 disabled:cursor-not-allowed disabled:opacity-60"
                            :disabled="busyIds.includes(quest.id)"
                            @click="stopGame(quest)"
                        >
                            {{ busyIds.includes(quest.id) ? t('quests.stopping') : t('quests.stop') }}
                        </button>
                        <button
                            v-else
                            type="button"
                            class="shrink-0 rounded-md bg-indigo-600 px-2.5 py-1 text-[11px] font-semibold text-white hover:bg-indigo-500 disabled:cursor-not-allowed disabled:bg-gray-200 disabled:text-gray-500 dark:disabled:bg-gray-700 dark:disabled:text-gray-400"
                            :disabled="!canCreate(quest) || busyIds.includes(quest.id)"
                            @click="createGame(quest)"
                        >
                            {{ busyIds.includes(quest.id) ? t('quests.creating') : t('quests.create') }}
                        </button>
                    </div>
                </div>
            </article>
        </div>

        <div v-if="hasLoaded" class="mt-5 space-y-1 text-xs text-gray-500 dark:text-gray-400">
            <p v-if="streamOnlyCount > 0">
                {{ t('quests.streamHidden').replace('{count}', String(streamOnlyCount)) }}
            </p>
            <p v-if="sources.length > 0">{{ t('quests.sources') }}: {{ sources.join(', ') }}</p>
            <p v-if="loadedAt">{{ t('quests.updatedAt') }}: {{ loadedAt.toLocaleTimeString() }}</p>
            <p v-for="sourceError in sourceErrors" :key="sourceError" class="text-amber-600 dark:text-amber-400">
                {{ sourceError }}
            </p>
        </div>
    </div>
</template>
