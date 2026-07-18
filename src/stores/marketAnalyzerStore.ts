import { create } from "zustand";

interface MarketAnalyzerState {
  pendingUrl: string | null;
  setPendingUrl: (url: string) => void;
  /** Reads and clears in one step — consume-once, so re-focusing the
   * Market Analyzer panel later (e.g. via workspace switch) doesn't
   * re-trigger a stale analysis. */
  consumePendingUrl: () => string | null;
}

export const useMarketAnalyzerStore = create<MarketAnalyzerState>((set, get) => ({
  pendingUrl: null,
  setPendingUrl: (url) => set({ pendingUrl: url }),
  consumePendingUrl: () => {
    const url = get().pendingUrl;
    set({ pendingUrl: null });
    return url;
  },
}));
