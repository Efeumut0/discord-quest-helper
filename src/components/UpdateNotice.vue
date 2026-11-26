<script setup lang="ts">
import { onMounted, onUnmounted, shallowRef } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useGlobalState } from '@/composables/state';

interface UpdateStatus {
    phase: string;
    version: string | null;
    error: string | null;
}

const { t, addLog } = useGlobalState();
const status = shallowRef<UpdateStatus>({ phase: 'idle', version: null, error: null });
let unlisten: UnlistenFn | undefined;
let disposed = false;
let receivedEvent = false;

function receive(value: UpdateStatus) {
    status.value = value;
    if (value.phase === 'error') addLog('warning', `${t('updates.failed')}: ${value.error}`);
}

onMounted(async () => {
    try {
        const stop = await listen<UpdateStatus>('app-update-status', event => {
            receivedEvent = true;
            receive(event.payload);
        });
        if (disposed) { stop(); return; }
        unlisten = stop;
        const value = await invoke<UpdateStatus>('get_update_status');
        if (!disposed && !receivedEvent) receive(value);
    } catch (error) {
        if (!disposed) addLog('warning', `${t('updates.failed')}: ${String(error)}`);
    }
});

onUnmounted(() => { disposed = true; unlisten?.(); });
</script>

<template>
    <div v-if="status.phase === 'downloading' || status.phase === 'installing'"
        class="border-b border-indigo-200 bg-indigo-50 px-4 py-2 text-center text-sm text-indigo-900 dark:border-indigo-800 dark:bg-indigo-950 dark:text-indigo-100"
        role="status" aria-live="polite">
        {{ t(status.phase === 'installing' ? 'updates.installing' : 'updates.downloading').replace('{version}', status.version || '') }}
    </div>
</template>
