<script setup lang="ts">
import { ref } from 'vue';
import { MoveHorizontal, MoveVertical } from '@lucide/vue';
import { useGlobalState } from '@/composables/state';
import { useWallpaper } from '@/composables/wallpaper';

const { t } = useGlobalState();
const { image, placement, ready, storageError, save } = useWallpaper();
const busy = ref(false);
const error = ref('');
const picker = ref<HTMLInputElement | null>(null);

const controls = [
    { key: 'x', label: 'horizontal', min: -100, max: 200 },
    { key: 'y', label: 'vertical', min: -100, max: 200 },
    { key: 'size', label: 'size', min: 10, max: 200 },
    { key: 'opacity', label: 'opacity', min: 0, max: 40 },
] as const;

async function updateImage(event?: Event) {
    const input = event?.target as HTMLInputElement | undefined;
    const file = input?.files?.[0];
    if (input) input.value = '';
    if (busy.value || (input && !file)) return;
    error.value = '';
    busy.value = true;
    try {
        if (!file) {
            await save();
            return;
        }

        const header = new Uint8Array(await file.slice(0, 8).arrayBuffer());
        const matches = (bytes: number[]) => bytes.every((byte, index) => byte === header[index]);
        const gifHeader = String.fromCharCode(...header.slice(0, 6));
        const type = matches([137, 80, 78, 71, 13, 10, 26, 10]) ? 'image/png'
            : matches([255, 216, 255]) ? 'image/jpeg'
            : gifHeader === 'GIF87a' || gifHeader === 'GIF89a' ? 'image/gif'
            : '';

        if (!type) {
            error.value = 'wallpaper.invalid';
            return;
        }

        const blob = file.slice(0, file.size, type);
        const previewUrl = URL.createObjectURL(blob);
        try {
            const preview = new Image();
            preview.src = previewUrl;
            await preview.decode();
        } catch {
            error.value = 'wallpaper.invalid';
            return;
        } finally {
            URL.revokeObjectURL(previewUrl);
        }

        await save(blob);
    } catch {
        error.value = 'wallpaper.error';
    } finally {
        busy.value = false;
    }
}
</script>

<template>
    <section class="rounded-lg border border-gray-200 bg-white p-5 dark:border-gray-700 dark:bg-gray-800">
        <h2 class="text-lg font-semibold text-gray-900 dark:text-white">{{ t('wallpaper.title') }}</h2>
        <div class="mt-3 flex flex-wrap items-center gap-3">
            <input
                ref="picker" type="file" class="hidden" accept="image/png,image/jpeg,image/gif,.png,.jpg,.jpeg,.gif"
                @change="updateImage($event)"
            />
            <button
                type="button" class="rounded-md bg-indigo-600 px-3 py-2 text-sm font-semibold text-white hover:bg-indigo-500 disabled:opacity-50"
                :disabled="busy || !ready" @click="picker?.click()"
            >{{ t('wallpaper.choose') }}</button>
            <button
                v-if="image" type="button"
                class="rounded-md border border-gray-300 px-3 py-2 text-sm font-medium text-gray-800 hover:bg-gray-100 dark:border-gray-600 dark:text-white dark:hover:bg-gray-700"
                :disabled="busy" @click="updateImage()"
            >{{ t('wallpaper.remove') }}</button>
        </div>
        <p v-if="error || storageError" role="alert" class="mt-2 text-sm text-red-700 dark:text-red-300">
            {{ t(error || 'wallpaper.error') }}
        </p>
        <div v-if="image" class="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-2">
            <label v-for="control in controls" :key="control.key" class="text-sm text-gray-700 dark:text-gray-300">
                <span class="flex justify-between gap-2">
                    <span>{{ t('wallpaper.' + control.label) }}</span>
                    <span class="inline-flex items-center gap-1 tabular-nums">
                        <span
                            v-if="(control.key === 'x' || control.key === 'y') && (placement[control.key] < 0 || placement[control.key] > 100)"
                            :title="t('wallpaper.outside')"
                        >
                            <component :is="control.key === 'x' ? MoveHorizontal : MoveVertical" class="size-4" :aria-label="t('wallpaper.outside')" />
                        </span>
                        {{ placement[control.key] }}%
                    </span>
                </span>
                <input
                    v-model.number="placement[control.key]" type="range" :min="control.min" :max="control.max"
                    class="mt-1 block w-full accent-indigo-600"
                />
            </label>
        </div>
    </section>
</template>
