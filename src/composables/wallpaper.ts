import { createGlobalState, useStorage } from '@vueuse/core';
import { computed, shallowRef } from 'vue';

export const useWallpaper = createGlobalState(() => {
    const placement = useStorage('wallpaper-placement', { x: 50, y: 50, size: 70, opacity: 12 });
    const image = shallowRef('');
    const ready = shallowRef(false);
    const storageError = shallowRef(false);
    const style = computed(() => {
        const { x, y, size, opacity } = placement.value;
        const horizontal = (x + 100) / 300;
        const vertical = (y + 100) / 300;
        // At either endpoint, keep just 2% of the image inside that edge.
        const offset = (position: number) => (-98 + position * 96) + '%';
        return {
            left: horizontal * 100 + '%',
            top: vertical * 100 + '%',
            width: size + '%',
            opacity: opacity / 100,
            transform: 'translate(' + offset(horizontal) + ', ' + offset(vertical) + ')',
        };
    });

    const database = new Promise<IDBDatabase>((resolve, reject) => {
        const request = indexedDB.open('quest-helper-wallpaper', 1);
        request.onupgradeneeded = () => request.result.createObjectStore('images');
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
    });

    function display(blob?: Blob) {
        if (image.value) URL.revokeObjectURL(image.value);
        image.value = blob ? URL.createObjectURL(blob) : '';
    }

    async function load() {
        try {
            const db = await database;
            const blob = await new Promise<Blob | undefined>((resolve, reject) => {
                const request = db.transaction('images').objectStore('images').get('wallpaper');
                request.onsuccess = () => resolve(request.result);
                request.onerror = () => reject(request.error);
            });
            display(blob);
            ready.value = true;
        } catch {
            storageError.value = true;
        }
    }

    async function save(blob?: Blob) {
        const db = await database;
        await new Promise<void>((resolve, reject) => {
            const transaction = db.transaction('images', 'readwrite');
            const store = transaction.objectStore('images');
            if (blob) store.put(blob, 'wallpaper');
            else store.delete('wallpaper');
            transaction.oncomplete = () => resolve();
            transaction.onabort = () => reject(transaction.error);
        });
        display(blob);
    }

    void load();
    return { placement, image, ready, storageError, style, save };
});
