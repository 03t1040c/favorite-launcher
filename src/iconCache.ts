export const ICON_CACHE_LIMIT = 512;
const ICON_CACHE_BYTES = 16 * 1024 * 1024;

// Plain immutable records keep React rendering predictable. Old entries are
// discarded; favorites themselves and native Windows icons are never modified.
export function rememberIcon(current: Record<string, string>, key: string, icon: string): Record<string, string> {
  if ((key.length + icon.length) * 2 > ICON_CACHE_BYTES) return current;
  const next = { ...current };
  delete next[key];
  next[key] = icon;
  let bytes = 0;
  const keys = Object.keys(next);
  for (let i = keys.length - 1; i >= 0; i--) {
    const entry = keys[i];
    bytes += (entry.length + next[entry].length) * 2;
    if (keys.length - i > ICON_CACHE_LIMIT || bytes > ICON_CACHE_BYTES) delete next[entry];
  }
  return next;
}
