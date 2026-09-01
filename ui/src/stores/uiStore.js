import { create } from 'zustand';
import { persist } from 'zustand/middleware';

export const useUiStore = create(
  persist(
    (set) => ({
      sidebarCollapsed: false,
      commandOpen: false,
      notificationsOpen: false,
      activeModal: null,
      setSidebarCollapsed: (v) => set({ sidebarCollapsed: !!v }),
      toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
      setCommandOpen: (v) => set({ commandOpen: !!v }),
      toggleCommand: () => set((s) => ({ commandOpen: !s.commandOpen })),
      setNotificationsOpen: (v) => set({ notificationsOpen: !!v }),
      openModal: (id) => set({ activeModal: id }),
      closeModal: () => set({ activeModal: null }),
    }),
    {
      name: 'stackhouse-ui-state',
      partialize: (s) => ({ sidebarCollapsed: s.sidebarCollapsed }),
    },
  ),
);
