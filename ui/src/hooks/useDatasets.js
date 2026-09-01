import { useEffect, useCallback } from 'react';
import { useDatasetStore } from '@/stores/datasetStore';
import { apiGet } from '@/lib/apiClient';

export function useDatasets({ auto = true } = {}) {
  const { datasets, activeDataset, loading, setDatasets, setActiveDataset, setLoading } = useDatasetStore();

  const fetchDatasets = useCallback(async () => {
    setLoading(true);
    try {
      const res = await apiGet('/v1/datasets');
      if (res.ok && res.data.success) {
        const list = res.data.datasets || [];
        setDatasets(list);
        if (list.length > 0 && !useDatasetStore.getState().activeDataset) {
          setActiveDataset(list[0].id);
        }
      }
    } catch (err) {
      console.error('Failed to load datasets', err);
    } finally {
      setLoading(false);
    }
  }, [setDatasets, setActiveDataset, setLoading]);

  useEffect(() => {
    if (auto) fetchDatasets();
  }, [auto, fetchDatasets]);

  return { datasets, activeDataset, loading, refresh: fetchDatasets, setActiveDataset };
}
