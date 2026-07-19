import { create } from "zustand";
import type { BackpackItem } from "../features/backpack/api";

interface SendOfferState {
  /** The items selected in the Backpack before "Propose Trade…" was
   * clicked — `null` means the form is closed. */
  giveItems: BackpackItem[] | null;
  open: (items: BackpackItem[]) => void;
  close: () => void;
}

export const useSendOfferStore = create<SendOfferState>((set) => ({
  giveItems: null,
  open: (items) => set({ giveItems: items }),
  close: () => set({ giveItems: null }),
}));
