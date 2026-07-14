import { create } from "zustand";

interface ContextMenuState {
  assetId: string;
  x: number;
  y: number;
}

interface BackpackState {
  selected: Set<string>;
  contextMenu: ContextMenuState | null;
  /** Ctrl/Cmd-click toggles membership and keeps the rest of the selection; a plain click replaces it. */
  selectItem: (assetId: string, additive: boolean) => void;
  clearSelection: () => void;
  openContextMenu: (menu: ContextMenuState) => void;
  closeContextMenu: () => void;
}

export const useBackpackStore = create<BackpackState>((set) => ({
  selected: new Set(),
  contextMenu: null,
  selectItem: (assetId, additive) =>
    set((state) => {
      if (!additive) {
        return { selected: new Set([assetId]) };
      }
      const next = new Set(state.selected);
      if (next.has(assetId)) {
        next.delete(assetId);
      } else {
        next.add(assetId);
      }
      return { selected: next };
    }),
  clearSelection: () => set({ selected: new Set() }),
  openContextMenu: (menu) => set({ contextMenu: menu }),
  closeContextMenu: () => set({ contextMenu: null }),
}));
