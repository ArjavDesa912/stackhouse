import { create } from 'zustand';

const MAX = 50;

export const useNotificationStore = create((set, get) => ({
  events: [],
  unread: 0,
  add: (event) => {
    const next = [{ id: crypto.randomUUID?.() || `${Date.now()}-${Math.random()}`, ts: Date.now(), ...event }, ...get().events].slice(0, MAX);
    set({ events: next, unread: get().unread + 1 });
  },
  markAllRead: () => set({ unread: 0 }),
  clear: () => set({ events: [], unread: 0 }),
}));
