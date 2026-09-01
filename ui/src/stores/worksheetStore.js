import { create } from 'zustand';

const DEFAULT_CONFIG = {
  columns: [],
  rows: [],
  marks: [],
  filters: [],
  showLegend: true,
  showGrid: true,
  colorScheme: 'default',
  chartType: 'auto',
};

const INITIAL_SHEETS = [{ id: 'sheet_1', name: 'Sheet 1', config: { ...DEFAULT_CONFIG } }];

export const useWorksheetStore = create((set, get) => ({
  sheets: INITIAL_SHEETS,
  activeSheetId: 'sheet_1',
  history: [],
  historyIndex: -1,

  getActiveSheet: () => {
    const { sheets, activeSheetId } = get();
    return sheets.find((s) => s.id === activeSheetId) || sheets[0];
  },

  setActiveSheetId: (id) => set({ activeSheetId: id }),

  pushHistory: () => {
    const { sheets, activeSheetId, history, historyIndex } = get();
    const next = historyIndex < history.length - 1 ? history.slice(0, historyIndex + 1) : history;
    next.push(JSON.stringify({ sheets, activeSheetId }));
    set({ history: next, historyIndex: next.length - 1 });
  },

  updateActiveConfig: (key, value) => {
    const { sheets, activeSheetId } = get();
    const nextSheets = sheets.map((s) =>
      s.id === activeSheetId ? { ...s, config: { ...s.config, [key]: value } } : s,
    );
    set({ sheets: nextSheets });
    get().pushHistory();
  },

  addSheet: () => {
    const { sheets } = get();
    const id = `sheet_${Date.now()}`;
    const next = [...sheets, { id, name: `Sheet ${sheets.length + 1}`, config: { ...DEFAULT_CONFIG } }];
    set({ sheets: next, activeSheetId: id });
    get().pushHistory();
  },

  deleteSheet: (id) => {
    const { sheets, activeSheetId } = get();
    if (sheets.length === 1) return;
    const next = sheets.filter((s) => s.id !== id);
    set({
      sheets: next,
      activeSheetId: activeSheetId === id ? next[0].id : activeSheetId,
    });
    get().pushHistory();
  },

  renameActive: (name) => {
    const { sheets, activeSheetId } = get();
    set({
      sheets: sheets.map((s) => (s.id === activeSheetId ? { ...s, name } : s)),
    });
    get().pushHistory();
  },

  loadConfig: (config) => {
    const { sheets, activeSheetId } = get();
    const cfg = config ? { ...DEFAULT_CONFIG, ...config } : { ...DEFAULT_CONFIG };
    const next = sheets.map((s) => (s.id === activeSheetId ? { ...s, config: cfg } : s));
    set({ sheets: next });
    get().pushHistory();
  },

  undo: () => {
    const { history, historyIndex } = get();
    if (historyIndex <= 0) return;
    const idx = historyIndex - 1;
    const snap = JSON.parse(history[idx]);
    set({ sheets: snap.sheets, activeSheetId: snap.activeSheetId, historyIndex: idx });
  },

  redo: () => {
    const { history, historyIndex } = get();
    if (historyIndex >= history.length - 1) return;
    const idx = historyIndex + 1;
    const snap = JSON.parse(history[idx]);
    set({ sheets: snap.sheets, activeSheetId: snap.activeSheetId, historyIndex: idx });
  },
}));

export { DEFAULT_CONFIG };
