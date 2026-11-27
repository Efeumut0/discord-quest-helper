import { type AppLanguage, TRANSLATIONS } from './i18n'
import { type AppTheme, isAppTheme, applyAppTheme } from './theme'
export * from './i18n'
export * from './theme'
import { createGlobalState } from '@vueuse/core'
import { shallowRef } from 'vue'
import type { ShallowRef } from 'vue'

export const SEARCH_PRIORITY_TARGET_DEFAULT = 2000
export const SEARCH_PRIORITY_TARGET_MIN = 100
export const SEARCH_PRIORITY_TARGET_MAX = 5000

function clampSearchPriorityTargetCount(value: number) {
    if (!Number.isFinite(value)) return SEARCH_PRIORITY_TARGET_DEFAULT
    return Math.min(
        SEARCH_PRIORITY_TARGET_MAX,
        Math.max(SEARCH_PRIORITY_TARGET_MIN, Math.round(value)),
    )
}
export const Pages = {
    HOME: 'home',
    ACTIVE_QUESTS: 'active-quests',
    GAMES: 'games',
    BULK_CREATE: 'bulk-create',
    GENERATED_GAMES: 'generated-games',
    PLAYGROUND: 'playground',
    SETTINGS: 'settings',
} as const
export type Pages = typeof Pages[keyof typeof Pages]

export interface AppLogObject {
    type: 'info' | 'error' | 'warning' | 'debug';
    message: string;
    timestamp: Date;
}
export interface UseGlobalStateReturn {
    page: ShallowRef<Pages>,
    setPage: (newPage: Pages) => void,
    logs: ShallowRef<AppLogObject[]>,
    addLog: {
        (type: 'info' | 'error' | 'warning' | 'debug', newLog: string): void;
        (newLog: string): void;
    };
    clearLogs: () => void,
    language: ShallowRef<AppLanguage>,
    setLanguage: (newLanguage: AppLanguage) => void,
    theme: ShallowRef<AppTheme>,
    setTheme: (newTheme: AppTheme) => void,
    searchPriorityTargetCount: ShallowRef<number>,
    setSearchPriorityTargetCount: (newCount: number) => void,
    promoPresence: ShallowRef<boolean>,
    setPromoPresence: (enabled: boolean) => void,
    t: (key: string) => string,
}
export const useGlobalState = createGlobalState(
  () => {
    const page = shallowRef<Pages>(Pages.HOME)
    const savedLanguage = localStorage.getItem('app-language') as AppLanguage | null
    const language = shallowRef<AppLanguage>(savedLanguage === 'tr' || savedLanguage === 'en' ? savedLanguage : 'tr')
    const savedTheme = localStorage.getItem('app-theme')
    const theme = shallowRef<AppTheme>(isAppTheme(savedTheme) ? savedTheme : 'current')
    applyAppTheme(theme.value)
    const savedSearchPriorityTargetCount = Number(localStorage.getItem('search-priority-target-count'))
    const searchPriorityTargetCount = shallowRef(
      clampSearchPriorityTargetCount(savedSearchPriorityTargetCount || SEARCH_PRIORITY_TARGET_DEFAULT),
    )

    // Promotional Rich Presence is on unless the user turned it off.
    const promoPresence = shallowRef<boolean>(localStorage.getItem('promo-presence') !== 'off')

    const logs = shallowRef<AppLogObject[]>([])

    function setPage(newPage: Pages) {
      page.value = newPage
    }

    function setLanguage(newLanguage: AppLanguage) {
      language.value = newLanguage
      localStorage.setItem('app-language', newLanguage)
    }

    function setTheme(newTheme: AppTheme) {
      theme.value = newTheme
      localStorage.setItem('app-theme', newTheme)
      applyAppTheme(newTheme)
    }

    function setSearchPriorityTargetCount(newCount: number) {
      const normalizedCount = clampSearchPriorityTargetCount(newCount)
      searchPriorityTargetCount.value = normalizedCount
      localStorage.setItem('search-priority-target-count', String(normalizedCount))
    }

    function setPromoPresence(enabled: boolean) {
      promoPresence.value = enabled
      localStorage.setItem('promo-presence', enabled ? 'on' : 'off')
    }

    function t(key: string) {
      return TRANSLATIONS[language.value][key] ?? TRANSLATIONS.en[key] ?? key
    }

    function addLog(type: string | 'info' | 'error' | 'warning' | 'debug' , newLog?: string) {
      if (!newLog) {
        newLog = type;
        type = 'info';
      }
      const formattedLog = `${newLog}`;
      logs.value = [...logs.value, { type: type as 'info' | 'error' | 'warning' | 'debug', message: formattedLog, timestamp: new Date() }];
    }

    function clearLogs() {
      logs.value = []
    }

    return {
        page,
        setPage,
        logs,
        addLog,
        clearLogs,
        language,
        setLanguage,
        theme,
        setTheme,
        searchPriorityTargetCount,
        setSearchPriorityTargetCount,
        promoPresence,
        setPromoPresence,
        t
    } as UseGlobalStateReturn
  }
)
