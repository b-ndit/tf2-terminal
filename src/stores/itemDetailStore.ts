import { create } from "zustand";
import type { DetailableItem } from "../features/item-detail/types";

interface ItemDetailState {
  item: DetailableItem | null;
  open: (item: DetailableItem) => void;
  close: () => void;
}

export const useItemDetailStore = create<ItemDetailState>((set) => ({
  item: null,
  open: (item) => set({ item }),
  close: () => set({ item: null }),
}));
