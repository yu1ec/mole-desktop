import { useCallback, useEffect, useRef, useState } from "react";

type CacheEntry<T> = {
  data?: T;
  updatedAt: number;
  promise?: Promise<T>;
};

// Session-wide cache: navigating away from a page no longer throws away scan results.
const store = new Map<string, CacheEntry<unknown>>();

export function readCache<T>(key: string): { data: T; updatedAt: number } | undefined {
  const entry = store.get(key) as CacheEntry<T> | undefined;
  if (!entry || entry.data === undefined) return undefined;
  return { data: entry.data, updatedAt: entry.updatedAt };
}

export function fetchCached<T>(key: string, fetcher: () => Promise<T>): Promise<T> {
  const entry = (store.get(key) as CacheEntry<T> | undefined) ?? { updatedAt: 0 };
  if (entry.promise) return entry.promise;
  store.set(key, entry);
  const promise = fetcher().then(
    (data) => {
      entry.data = data;
      entry.updatedAt = Date.now();
      entry.promise = undefined;
      return data;
    },
    (error) => {
      entry.promise = undefined;
      throw error;
    },
  );
  entry.promise = promise;
  return promise;
}

export function invalidateCachePrefix(prefix: string) {
  for (const key of store.keys()) {
    if (key.startsWith(prefix)) store.delete(key);
  }
}

export interface CachedQueryResult<T> {
  data: T | undefined;
  /** True only while the first result is on its way (no cached data yet). */
  loading: boolean;
  /** True while a background refresh is running, even if cached data is shown. */
  refreshing: boolean;
  error: string;
  refresh: (options?: { force?: boolean }) => Promise<T | undefined>;
}

/**
 * Stale-while-revalidate for a Tauri command.
 *
 * The cached value is returned synchronously on mount, so pages render
 * immediately. A background refresh runs when the value is older than `ttl`;
 * explicit refresh actions can force it.
 */
export function useCachedQuery<T>(
  key: string | null,
  fetcher: () => Promise<T>,
  options: { ttl?: number; enabled?: boolean } = {},
): CachedQueryResult<T> {
  const { ttl = 30_000, enabled = true } = options;
  const active = !!key && enabled;
  const cached = active ? readCache<T>(key) : undefined;

  const [state, setState] = useState<{ key: string | null; data?: T }>({
    key,
    data: cached?.data,
  });
  const [loading, setLoading] = useState(active && !cached);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState("");

  const keyRef = useRef(key);
  keyRef.current = key;
  const fetcherRef = useRef(fetcher);
  fetcherRef.current = fetcher;
  const ttlRef = useRef(ttl);
  ttlRef.current = ttl;

  const refresh = useCallback(
    async (opts?: { force?: boolean }) => {
      if (!key || !enabled) return undefined;
      const current = readCache<T>(key);
      if (!opts?.force && current && Date.now() - current.updatedAt < ttlRef.current) {
        setState({ key, data: current.data });
        setLoading(false);
        return current.data;
      }
      if (!current) setLoading(true);
      setRefreshing(true);
      setError("");
      try {
        const data = await fetchCached(key, () => fetcherRef.current());
        if (keyRef.current === key) setState({ key, data });
        return data;
      } catch (err) {
        if (keyRef.current === key) setError(String(err));
        return undefined;
      } finally {
        if (keyRef.current === key) {
          setLoading(false);
          setRefreshing(false);
        }
      }
    },
    [key, enabled],
  );

  useEffect(() => {
    if (!key || !enabled) {
      setState({ key, data: undefined });
      setLoading(false);
      setRefreshing(false);
      setError("");
      return;
    }
    const current = readCache<T>(key);
    setState({ key, data: current?.data });
    setLoading(!current);
    setRefreshing(false);
    setError("");
    void refresh();
  }, [key, enabled, refresh]);

  // Ignore stale values from the previous key for the first render after a switch.
  const isCurrent = state.key === key;
  return {
    data: isCurrent ? state.data : cached?.data,
    loading: isCurrent ? loading : active && !cached,
    refreshing: isCurrent ? refreshing : false,
    error: isCurrent ? error : "",
    refresh,
  };
}
