<script setup lang="ts">
import { onDeactivated, onMounted, ref } from 'vue';
import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { useGlobalState } from '@/composables/state';

const ActivityKind = {
    Playing: 0,
    Listening: 2,
    Watching: 3,
    Competing: 5
} as const;

const version = ref('');
onMounted(async () => {
    try {
        version.value = await getVersion();
    } catch (error) {
        addLog('error', errorMessage(error));
    }
});

const isConnected = ref(false);
const isTesting = ref(false);
const appIdCode = '1361728268088381706';

const { logs, addLog, clearLogs, t } = useGlobalState();

function errorMessage(error: unknown) {
    return error instanceof Error ? error.message : String(error);
}

let stopRequested = false;

async function disconnectTest() {
    await invoke('connect_to_discord_rpc_3', {
        activity_json: '{}',
        action: 'disconnect',
    });
    isConnected.value = false;
}

async function stopTest() {
    stopRequested = true;
    if (isTesting.value || !isConnected.value) return;

    isTesting.value = true;
    try {
        await disconnectTest();
    } catch (error) {
        addLog('error', `${t('playground.connectionFailed')} ${errorMessage(error)}`);
    } finally {
        isTesting.value = false;
    }
}

onDeactivated(() => { void stopTest(); });

async function discordTest() {
    if (isTesting.value) return;
    if (isConnected.value) {
        await stopTest();
        return;
    }

    stopRequested = false;
    isTesting.value = true;
    try {
        await invoke('connect_to_discord_rpc_3', {
            activity_json: JSON.stringify({
                app_id: appIdCode,
                details: t('playground.testDetails'),
                state: version.value ? `Quest Helper v${version.value}` : 'Quest Helper',
                activity_kind: ActivityKind.Playing,
                timestamp: Math.floor(Date.now() / 1000)
            }),
            action: 'connect',
        });
        isConnected.value = true;
        if (stopRequested) await disconnectTest();
        else addLog('info', t('playground.testSent'));
    } catch (error) {
        addLog('error', `${t('playground.connectionFailed')} ${errorMessage(error)}`);
    } finally {
        isTesting.value = false;
    }
}

</script>

<template>
    <div class="app-page app-page-full app-page-wide flex-col gap-5">
        <section class="relative shrink-0 overflow-hidden rounded-lg border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-800">
            <div class="playground-grid absolute inset-0"></div>
            <div class="relative grid gap-5 p-5 md:grid-cols-[1fr_auto] md:items-center">
                <div class="flex flex-col gap-5 sm:flex-row sm:items-center">
                    <div class="relative flex h-28 w-28 shrink-0 items-center justify-center">
                        <div class="absolute inset-0 rotate-3 rounded-lg border border-indigo-300/40 bg-indigo-500/10 dark:border-indigo-400/20 dark:bg-indigo-500/10"></div>
                        <div class="absolute inset-3 -rotate-2 rounded-lg border border-gray-200 bg-gray-50 shadow-inner dark:border-gray-700 dark:bg-gray-900"></div>
                        <img src="/logo.svg" alt="Logo" class="relative h-20 w-20 drop-shadow-lg" />
                    </div>

                    <div class="min-w-0">
                        <div class="mb-2 text-xs font-semibold uppercase text-indigo-500 dark:text-indigo-300">
                            {{ t('playground.console') }}
                        </div>
                        <h1 class="text-2xl font-bold text-gray-900 dark:text-white sm:text-3xl">
                            Discord Quest Helper
                        </h1>
                        <p v-if="version" class="mt-1 text-sm text-gray-500 dark:text-gray-400">v{{ version }}</p>
                        <div class="mt-4 flex flex-wrap gap-2 text-xs">
                            <span
                                class="rounded-md border px-2.5 py-1 font-medium"
                                :class="isConnected
                                    ? 'border-green-500/40 bg-green-500/10 text-green-600 dark:text-green-300'
                                    : 'border-gray-300 bg-gray-100 text-gray-600 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-300'"
                            >
                                {{ isConnected ? t('playground.connected') : t('playground.idle') }}
                            </span>
                            <span class="rounded-md border border-gray-200 bg-gray-50 px-2.5 py-1 font-medium text-gray-600 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-300">
                                {{ t('playground.signal') }} RPC
                            </span>
                            <span class="rounded-md border border-gray-200 bg-gray-50 px-2.5 py-1 font-mono text-gray-600 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-300">
                                {{ t('playground.appId') }} {{ appIdCode }}
                            </span>
                        </div>
                    </div>
                </div>

                <button
                    class="rounded-md px-5 py-2.5 text-sm font-semibold text-white shadow-sm transition-colors"
                    :class="isConnected ? 'bg-red-600 hover:bg-red-500' : 'bg-indigo-600 hover:bg-indigo-500'"
                    :disabled="isTesting"
                    @click="discordTest"
                >
                    {{ isConnected ? t('playground.stopTest') : t('playground.discordTest') }}
                </button>
            </div>
        </section>

        <div class="flex min-h-0 flex-1 flex-col rounded-lg border border-gray-200 bg-white p-4 text-gray-700 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-300">
            <div class="mb-2 flex shrink-0 items-center justify-between">
                <h2 class="text-lg font-semibold mb-2">{{ t('playground.logs') }}</h2>
                <button class="mt-2 rounded-md bg-gray-200 px-3 py-1 text-sm font-bold hover:bg-gray-300 dark:bg-gray-700 dark:hover:bg-gray-600"
                    @click="clearLogs">{{ t('playground.clearLogs') }}</button>
            </div>

            <div class="min-h-[220px] flex-1 overflow-y-auto rounded p-2">

                <div v-if="logs.length === 0" class="text-gray-400">{{ t('playground.noLogs') }}</div>
                <ul v-else class="list-none">
                    <li v-for="(log, index) in logs" :key="index" class="text-sm">
                        <span class="text-gray-500">[{{ new Date(log.timestamp).toLocaleString() }}]</span>
                        <span :class="{
                            'text-blue-400': log.type === 'info',
                            'text-red-400': log.type === 'error',
                            'text-yellow-400': log.type === 'warning',
                            'text-green-400': log.type === 'debug'
                        }">
                            [{{ log.type.toUpperCase() }}]
                        </span>
                        <span class="ml-1">{{ log.message }}</span>
                    </li>
                </ul>
            </div>

        </div>
    </div>
</template>

<style scoped>
.playground-grid {
    background-image:
        linear-gradient(rgba(99, 102, 241, 0.12) 1px, transparent 1px),
        linear-gradient(90deg, rgba(99, 102, 241, 0.12) 1px, transparent 1px);
    background-size: 24px 24px;
    mask-image: linear-gradient(to bottom right, black, transparent 78%);
}
</style>
