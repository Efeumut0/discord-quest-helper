import { createGlobalState } from '@vueuse/core'
import { shallowRef, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useGlobalState } from './state'

export interface PromoPresenceStatus {
    phase: string
    error: string | null
}

const STATUS_KEYS: Record<string, string> = {
    disabled: 'settings.promoStatusDisabled',
    connecting: 'settings.promoStatusConnecting',
    active: 'settings.promoStatusActive',
    suspended: 'settings.promoStatusSuspended',
    unavailable: 'settings.promoStatusUnavailable',
}

export function promoStatusKey(phase: string) {
    return STATUS_KEYS[phase] ?? STATUS_KEYS.disabled
}

export const usePromoPresence = createGlobalState(() => {
    const { language } = useGlobalState()
    const phase = shallowRef<string>('disabled')
    let started = false

    async function sync() {
        try {
            await invoke('set_promo_presence', {
                enabled: true,
                language: language.value,
            })
        } catch (error) {
            console.error('Promo presence could not be updated', error)
        }
    }

    async function start() {
        if (started) return
        started = true
        watch(language, () => {
            void sync()
        })
        await sync()
        try {
            const status = await invoke<PromoPresenceStatus>('get_promo_presence_status')
            phase.value = status.phase
        } catch (error) {
            console.error('Promo presence status unavailable', error)
        }
        void listen<PromoPresenceStatus>('promo-presence-status', (event) => {
            phase.value = event.payload.phase
        })
    }

    return { phase, start, sync }
})
