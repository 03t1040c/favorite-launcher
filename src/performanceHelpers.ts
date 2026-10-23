// Preserve React state references when a poll returned identical rows.
export function sameRows<T extends object>(previous: T[], next: T[]): boolean {
  return previous.length === next.length && previous.every((row, index) => {
    const other = next[index];
    const keys = Object.keys(row) as (keyof T)[];
    return keys.length === Object.keys(other).length && keys.every(key => row[key] === other[key]);
  });
}

// Serialize writes for the same record, while allowing unrelated records to
// progress independently. A rejected write must not poison later operations.
export function createKeyedTaskQueue<Key>() {
  const tails = new Map<Key, Promise<void>>();
  return (key: Key, task: () => Promise<void>): Promise<void> => {
    const result = (tails.get(key) ?? Promise.resolve()).then(task);
    const settled = result.then(() => {}, () => {});
    tails.set(key, settled);
    void settled.then(() => { if (tails.get(key) === settled) tails.delete(key); });
    return result;
  };
}
