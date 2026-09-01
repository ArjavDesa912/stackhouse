import { create } from 'zustand';

export const useDatasetStore = create((set) => ({
  datasets: [],
  activeDataset: '',
  loading: true,
  setDatasets: (datasets) => set({ datasets }),
  setActiveDataset: (id) => set({ activeDataset: id }),
  setLoading: (loading) => set({ loading: !!loading }),
}));
