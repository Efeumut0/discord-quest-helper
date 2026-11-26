<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useGlobalState } from '@/composables/state';

type BulkMode = 'all' | 'missing' | 'icons' | 'missing-icons';

interface BulkTaskStarted {
    mode: BulkMode;
}

interface BulkTaskOutput {
    stream: 'stdout' | 'stderr';
    line: string;
}

interface BulkTaskFinished {
    mode: BulkMode;
    success: boolean;
    exit_code: number | null;
    message: string;
}

interface BulkTaskAvailability {
    available: boolean;
    message: string;
}

interface BulkProgress {
    activity: string;
    percent: number;
    current: number;
    total: number;
}

const { t } = useGlobalState();

const logs = ref<BulkTaskOutput[]>([]);
const isRunning = ref(false);
const currentMode = ref<BulkMode | null>(null);
const statusMessage = ref(t('bulk.statusIdle'));
const progress = ref<BulkProgress | null>(null);
const availability = ref<BulkTaskAvailability | null>(null);
const unlisteners: UnlistenFn[] = [];

const taskOptions: Array<{ mode: BulkMode; label: string; tone: string }> = [
    { mode: 'all', label: 'bulk.createAll', tone: 'bg-indigo-600 hover:bg-indigo-500' },
    { mode: 'missing', label: 'bulk.createMissing', tone: 'bg-gray-700 hover:bg-gray-600' },
    { mode: 'icons', label: 'bulk.refreshIcons', tone: 'bg-sky-600 hover:bg-sky-500' },
    { mode: 'missing-icons', label: 'bulk.fillMissingIcons', tone: 'bg-emerald-600 hover:bg-emerald-500' },
];

const currentModeLabel = computed(() => currentMode.value ? getModeLabel(currentMode.value) : '');
const isBulkAvailable = computed(() => availability.value?.available ?? true);

function formatMessage(template: string, values: Record<string, string | number>) {
    return Object.entries(values).reduce(
        (message, [key, value]) => message.replace(`{${key}}`, String(value)),
        template,
    );
}

function getModeLabel(mode: BulkMode) {
    if (mode === 'all') return t('bulk.modeAll');
    if (mode === 'missing') return t('bulk.modeMissing');
    if (mode === 'missing-icons') return t('bulk.modeMissingIcons');
    return t('bulk.modeIcons');
}

function parseProgressLine(line: string): BulkProgress | null {
    const sections = line.split('|');
    if (sections.length !== 5 || sections[0] !== '__GA_PROGRESS__') return null;

    const percent = Number(sections[2]);
    const current = Number(sections[3]);
    const total = Number(sections[4]);
    if (!Number.isFinite(percent) || !Number.isFinite(current) || !Number.isFinite(total)) {
        return null;
    }

    return {
        activity: sections[1],
        percent: Math.max(0, Math.min(100, percent)),
        current,
        total,
    };
}

function addLog(stream: BulkTaskOutput['stream'], line: string) {
    if (!line.trim()) return;

    const parsedProgress = parseProgressLine(line);
    if (parsedProgress) {
        progress.value = parsedProgress;
        logs.value.push({
            stream,
            line: `${parsedProgress.activity}: ${parsedProgress.percent}% (${parsedProgress.current} / ${parsedProgress.total})`,
        });
    } else {
        logs.value.push({ stream, line });
    }

    if (logs.value.length > 2000) {
        logs.value = logs.value.slice(-2000);
    }
}

async function startTask(mode: BulkMode) {
    if (isRunning.value || !isBulkAvailable.value) return;

    logs.value = [];
    progress.value = null;
    isRunning.value = true;
    currentMode.value = mode;
    statusMessage.value = formatMessage(t('bulk.statusRunning'), { mode: getModeLabel(mode) });

    try {
        await invoke('start_bulk_game_task', { mode });
    } catch (error) {
        isRunning.value = false;
        const message = `${t('bulk.statusFailed')} ${String(error)}`;
        statusMessage.value = message;
        addLog('stderr', message);
    }
}

function clearTaskLogs() {
    logs.value = [];
}

async function loadAvailability() {
    try {
        availability.value = await invoke<BulkTaskAvailability>('get_bulk_task_availability');
        if (!availability.value.available) {
            statusMessage.value = availability.value.message;
        }
    } catch {
        availability.value = null;
    }
}

onMounted(async () => {
    await loadAvailability();

    unlisteners.push(await listen<BulkTaskStarted>('bulk_game_task_started', (event) => {
        currentMode.value = event.payload.mode;
        isRunning.value = true;
        statusMessage.value = formatMessage(t('bulk.statusRunning'), {
            mode: getModeLabel(event.payload.mode),
        });
    }));

    unlisteners.push(await listen<BulkTaskOutput>('bulk_game_task_output', (event) => {
        addLog(event.payload.stream, event.payload.line);
    }));

    unlisteners.push(await listen<BulkTaskFinished>('bulk_game_task_finished', (event) => {
        isRunning.value = false;
        currentMode.value = event.payload.mode;
        if (event.payload.success && progress.value) {
            progress.value = { ...progress.value, percent: 100 };
        }
        statusMessage.value = event.payload.success
            ? t('bulk.statusFinished')
            : `${t('bulk.statusFailed')} ${event.payload.message}`;
    }));
});

onUnmounted(() => {
    while (unlisteners.length) {
        unlisteners.pop()?.();
    }
});
</script>

<template>
    <div class="app-page app-page-full app-page-wide flex-col">
        <div class="mb-5 shrink-0 flex flex-col gap-2 md:flex-row md:items-end md:justify-between">
            <div>
                <h1 class="text-3xl font-bold text-gray-900 dark:text-white">
                    {{ t('bulk.title') }}
                </h1>
                <div class="mt-2 text-sm text-gray-500 dark:text-gray-400">
                    {{ statusMessage }}
                </div>
            </div>

            <div v-if="isRunning" class="rounded-md border border-indigo-500/40 bg-indigo-50 px-3 py-2 text-sm text-indigo-600 dark:bg-indigo-500/10 dark:text-indigo-300">
                {{ currentModeLabel }}
            </div>
        </div>

        <section v-if="progress" class="mb-5 shrink-0 rounded-lg border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-800">
            <div class="mb-2 flex items-center justify-between gap-3 text-sm">
                <div class="font-medium text-gray-800 dark:text-white">
                    {{ progress.activity }}
                </div>
                <div class="font-mono text-gray-500 dark:text-gray-400">
                    {{ progress.percent }}% ({{ progress.current }} / {{ progress.total }})
                </div>
            </div>
            <div class="h-3 overflow-hidden rounded-full bg-gray-200 dark:bg-gray-700">
                <div
                    class="h-full rounded-full bg-indigo-600 transition-all duration-300"
                    :style="{ width: `${progress.percent}%` }"
                ></div>
            </div>
        </section>

        <section class="mb-5 shrink-0 grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-4">
            <button
                v-for="option in taskOptions"
                :key="option.mode"
                type="button"
                class="rounded-lg px-5 py-4 text-left text-sm font-semibold text-white transition-colors disabled:cursor-not-allowed disabled:opacity-60"
                :class="option.tone"
                :disabled="isRunning || !isBulkAvailable"
                @click="startTask(option.mode)"
            >
                {{ t(option.label) }}
            </button>
        </section>

        <section class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-800">
            <div class="shrink-0 flex items-center justify-between border-b border-gray-200 px-5 py-4 dark:border-gray-700">
                <h2 class="text-lg font-semibold text-gray-900 dark:text-white">
                    {{ t('bulk.logs') }}
                </h2>
                <button
                    type="button"
                    class="rounded-md bg-gray-200 px-3 py-1 text-sm font-semibold text-gray-700 hover:bg-gray-300 dark:bg-gray-700 dark:text-gray-200 dark:hover:bg-gray-600"
                    @click="clearTaskLogs"
                >
                    {{ t('bulk.clearLogs') }}
                </button>
            </div>

            <div class="min-h-[180px] flex-1 overflow-y-auto bg-gray-50 p-4 font-mono text-xs dark:bg-gray-900">
                <div v-if="logs.length === 0" class="font-sans text-sm text-gray-500 dark:text-gray-400">
                    {{ t('bulk.noLogs') }}
                </div>
                <div
                    v-for="(log, index) in logs"
                    :key="index"
                    class="whitespace-pre-wrap break-words py-0.5"
                    :class="log.stream === 'stderr' ? 'text-red-400' : 'text-gray-700 dark:text-gray-300'"
                >
                    {{ log.line }}
                </div>
            </div>
        </section>
    </div>
</template>
