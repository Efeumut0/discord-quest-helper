export type PaginationItem = { type: 'page'; page: number } | { type: 'ellipsis'; key: string };

export function buildPaginationItems(currentPage: number, totalPages: number): PaginationItem[] {
    const total = Math.max(1, totalPages);
    const pages = new Set<number>();

    for (let page = 1; page <= Math.min(3, total); page++) {
        pages.add(page);
    }

    for (let page = currentPage - 1; page <= currentPage + 1; page++) {
        if (page >= 1 && page <= total) {
            pages.add(page);
        }
    }

    for (let page = Math.max(1, total - 1); page <= total; page++) {
        pages.add(page);
    }

    const sortedPages = Array.from(pages).sort((a, b) => a - b);
    const items: PaginationItem[] = [];
    let previousPage = 0;

    for (const page of sortedPages) {
        if (previousPage > 0 && page - previousPage > 1) {
            items.push({ type: 'ellipsis', key: `ellipsis-${previousPage}-${page}` });
        }
        items.push({ type: 'page', page });
        previousPage = page;
    }

    return items;
}
