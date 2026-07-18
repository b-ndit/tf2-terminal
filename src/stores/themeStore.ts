import { create } from "zustand";
import { persist } from "zustand/middleware";

export type ThemeName = "dark" | "light" | "oled";

interface ThemeState {
  theme: ThemeName;
  setTheme: (theme: ThemeName) => void;
}

export const useThemeStore = create<ThemeState>()(
  persist(
    (set) => ({
      theme: "dark",
      setTheme: (theme) => set({ theme }),
    }),
    { name: "tf2-terminal-theme" },
  ),
);

// localStorage rehydration for the `persist` middleware runs synchronously,
// so applying it once here (module load) plus on every future change avoids
// a flash of the default theme before React ever renders.
document.documentElement.dataset.theme = useThemeStore.getState().theme;
useThemeStore.subscribe((state) => {
  document.documentElement.dataset.theme = state.theme;
});
