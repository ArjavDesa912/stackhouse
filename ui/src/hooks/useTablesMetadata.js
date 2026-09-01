import { useEffect, useCallback } from 'react';
import { useTableStore } from '@/stores/tableStore';
import { apiGet } from '@/lib/apiClient';

export function useTablesMetadata({ auto = true } = {}) {
  const { tables, activeTable, loading, setTables, setActiveTable, setLoading } = useTableStore();

  const fetchMetadata = useCallback(async () => {
    setLoading(true);
    try {
      const res = await apiGet('/v1/tables');
      if (res.ok && res.data.success) {
        const statsResults = await Promise.all(
          res.data.tables.map((name) =>
            apiGet(`/v1/tables/${name}`).catch(() => null),
          ),
        );
        const detailedTables = statsResults.filter((r) => r && r.ok && r.data.success).map((r) => r.data.data);
        setTables(detailedTables);
        if (detailedTables.length > 0 && !useTableStore.getState().activeTable) {
          setActiveTable(detailedTables[0].name);
        }
      }
    } catch (err) {
      console.error('Failed to load metadata', err);
    } finally {
      setLoading(false);
    }
  }, [setTables, setActiveTable, setLoading]);

  useEffect(() => {
    if (auto) fetchMetadata();
  }, [auto, fetchMetadata]);

  return { tables, activeTable, loading, refresh: fetchMetadata, setActiveTable };
}
