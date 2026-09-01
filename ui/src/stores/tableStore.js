import { create } from 'zustand';

export const useTableStore = create((set) => ({
  tables: [],
  activeTable: '',
  loading: true,
  setTables: (tables) => set({ tables }),
  setActiveTable: (name) => set({ activeTable: name }),
  setLoading: (loading) => set({ loading: !!loading }),
}));
