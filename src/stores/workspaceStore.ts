import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { SerializedDockview } from "dockview-react";

export type PanelId =
  | "backpack"
  | "market-analyzer"
  | "trade-analyzer"
  | "live-feed"
  | "alerts"
  | "flip-finder"
  | "portfolio"
  | "trade-history"
  | "simulator"
  | "plugins";

export const PANEL_TITLES: Record<PanelId, string> = {
  backpack: "Backpack",
  "market-analyzer": "Market Analyzer",
  "trade-analyzer": "Trade Analyzer",
  "live-feed": "Live Feed",
  alerts: "Alerts",
  "flip-finder": "Flip Finder",
  portfolio: "Portfolio",
  "trade-history": "Trade History",
  simulator: "Simulator",
  plugins: "Plugins",
};

/**
 * Named per docs/DESIGN.md §9's worked examples ("Trading", "Portfolio",
 * "Sniping"), built only from panels that actually exist today — the
 * design doc's mockup also references a standalone "item detail" panel
 * that Module 7 folded into Market Analyzer instead, so it has no entry
 * here. Order matters: it's the left-to-right split order a fresh layout
 * is built in, and the index into this object also drives the `1`-`9`
 * workspace-switch keybind (docs/DESIGN.md §9).
 */
export const WORKSPACE_DEFAULTS: Record<string, PanelId[]> = {
  Trading: ["backpack", "market-analyzer", "live-feed", "trade-analyzer"],
  Portfolio: ["portfolio", "trade-history", "backpack"],
  Sniping: ["live-feed", "flip-finder", "market-analyzer", "alerts"],
};

export const WORKSPACE_NAMES = Object.keys(WORKSPACE_DEFAULTS);

interface WorkspaceState {
  activeLayoutName: string;
  /** Populated the first time a layout's panels are moved/resized; until
   * then a fresh layout is built from `WORKSPACE_DEFAULTS`. */
  layouts: Partial<Record<string, SerializedDockview>>;
  setActiveLayoutName: (name: string) => void;
  saveActiveLayout: (data: SerializedDockview) => void;
}

export const useWorkspaceStore = create<WorkspaceState>()(
  persist(
    (set) => ({
      activeLayoutName: WORKSPACE_NAMES[0],
      layouts: {},
      setActiveLayoutName: (name) => set({ activeLayoutName: name }),
      saveActiveLayout: (data) =>
        set((state) => ({
          layouts: { ...state.layouts, [state.activeLayoutName]: data },
        })),
    }),
    { name: "tf2-terminal-workspaces" },
  ),
);
