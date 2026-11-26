export type AppTheme = 'current' | 'dark' | 'light' | 'purple' | 'blue' | 'green' | 'red'

export const APP_THEME_OPTIONS: AppTheme[] = ['current', 'dark', 'light', 'purple', 'blue', 'green', 'red']

export function isAppTheme(value: string | null): value is AppTheme {
    return APP_THEME_OPTIONS.includes(value as AppTheme)
}

export function applyAppTheme(theme: AppTheme) {
    if (typeof document === 'undefined') return

    const root = document.documentElement
    root.classList.remove('theme-custom', ...APP_THEME_OPTIONS.map(option => `theme-${option}`))

    if (theme === 'current') {
        delete root.dataset.theme
        return
    }

    root.dataset.theme = theme
    root.classList.add('theme-custom', `theme-${theme}`)
}
