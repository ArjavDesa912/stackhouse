import { describe, it, expect, beforeEach } from 'vitest';
import { useWorksheetStore, DEFAULT_CONFIG } from './worksheetStore';

describe('worksheetStore', () => {
  beforeEach(() => {
    // Reset the store between tests
    useWorksheetStore.setState({
      sheets: [{ id: 'sheet_1', name: 'Sheet 1', config: { ...DEFAULT_CONFIG } }],
      activeSheetId: 'sheet_1',
      history: [],
      historyIndex: -1,
    });
  });

  it('updates active config and pushes history', () => {
    const { updateActiveConfig } = useWorksheetStore.getState();
    updateActiveConfig('chartType', 'bar');
    expect(useWorksheetStore.getState().getActiveSheet().config.chartType).toBe('bar');
    expect(useWorksheetStore.getState().history.length).toBe(1);
  });

  it('adds and deletes sheets', () => {
    const store = useWorksheetStore.getState();
    store.addSheet();
    expect(useWorksheetStore.getState().sheets.length).toBe(2);
    const newId = useWorksheetStore.getState().activeSheetId;
    useWorksheetStore.getState().deleteSheet(newId);
    expect(useWorksheetStore.getState().sheets.length).toBe(1);
  });

  it('undo restores previous state', () => {
    const { updateActiveConfig, undo } = useWorksheetStore.getState();
    updateActiveConfig('chartType', 'bar');
    updateActiveConfig('chartType', 'pie');
    undo();
    expect(useWorksheetStore.getState().getActiveSheet().config.chartType).toBe('bar');
  });
});
