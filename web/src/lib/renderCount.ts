// Counts how often a component draws. It is off unless a browser test turns
// it on by creating `window.__rdRenderCounts` before the page loads. The test
// uses the counts to check that moving 1 window does not redraw the others.

declare global {
  interface Window {
    __rdRenderCounts?: Record<string, number>
  }
}

export function countRender(name: string): void {
  const counts = window.__rdRenderCounts
  if (counts) counts[name] = (counts[name] ?? 0) + 1
}
