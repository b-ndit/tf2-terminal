import type { FC } from "react";
import type { IDockviewPanelProps } from "dockview-react";
import type { PanelId } from "../../stores/workspaceStore";
import { Alerts } from "../../features/alerts/Alerts";
import { BackpackPanel } from "../../features/backpack/BackpackPanel";
import { FlipFinder } from "../../features/flip-finder/FlipFinder";
import { LiveFeed } from "../../features/live-feed/LiveFeed";
import { MarketAnalyzer } from "../../features/market-analyzer/MarketAnalyzer";
import { Plugins } from "../../features/plugins/Plugins";
import { Portfolio } from "../../features/portfolio/Portfolio";
import { Simulator } from "../../features/simulator/Simulator";
import { TradeAnalyzer } from "../../features/trade-analyzer/TradeAnalyzer";
import { TradeHistory } from "../../features/trade-history/TradeHistory";

/**
 * id -> Dockview panel component. Every entry is self-sufficient (own
 * TanStack Query hooks, shared cache by query key) — Dockview mounts them
 * with `IDockviewPanelProps` (api/containerApi/params), which none of
 * these declare or need, so plain zero-arg components satisfy the type.
 */
export const panelComponents: Record<PanelId, FC<IDockviewPanelProps>> = {
  backpack: BackpackPanel,
  "market-analyzer": MarketAnalyzer,
  "trade-analyzer": TradeAnalyzer,
  "live-feed": LiveFeed,
  alerts: Alerts,
  "flip-finder": FlipFinder,
  portfolio: Portfolio,
  "trade-history": TradeHistory,
  simulator: Simulator,
  plugins: Plugins,
};
