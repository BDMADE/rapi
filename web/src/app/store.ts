import { create } from "zustand";

interface UiState {
  sidebarCollapsed: boolean;
  activeLocale: string;
  commandOpen: boolean;
  toggleSidebar: () => void;
  setActiveLocale: (locale: string) => void;
  setCommandOpen: (open: boolean) => void;
}

export const useUiStore = create<UiState>((set) => ({
  sidebarCollapsed: false,
  activeLocale: "en",
  commandOpen: false,
  toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
  setActiveLocale: (locale) => set({ activeLocale: locale }),
  setCommandOpen: (open) => set({ commandOpen: open }),
}));
