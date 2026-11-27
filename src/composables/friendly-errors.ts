// Backend and Discord errors arrive as developer text. The interface shows a
// plain sentence instead, and keeps the original line for the log.

const RULES: { key: string; match: RegExp }[] = [
    {
        key: 'errors.discordClosed',
        match: /discord (desktop )?(is )?not (connected|running)|failed to connect to discord|unable to create discord client|discord rpc|transport error|connection refused|pipe/i,
    },
    {
        key: 'errors.rateLimited',
        match: /http 429|too many requests|rate limit/i,
    },
    {
        key: 'errors.network',
        match: /error sending request|dns error|timed out|timeout|network|unreachable|http 5\d\d/i,
    },
    {
        key: 'errors.permission',
        match: /access is denied|os error 5|permission denied|erişim engellendi/i,
    },
    {
        key: 'errors.fileInUse',
        match: /os error 32|being used by another process|kullanımda/i,
    },
    {
        key: 'errors.diskFull',
        match: /os error 112|not enough space|disk full/i,
    },
]

export function friendlyErrorKey(raw: string): string | null {
    for (const rule of RULES) {
        if (rule.match.test(raw)) return rule.key
    }
    return null
}

/** Plain sentence for the user, with the original text kept after it. */
export function friendlyError(raw: string, t: (key: string) => string): string {
    const key = friendlyErrorKey(raw)
    if (!key) return raw
    return `${t(key)} (${raw})`
}
