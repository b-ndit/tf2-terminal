import { useState } from "react";
import { Alerts } from "./features/alerts/Alerts";
import { useLoginWithSteam, useLogoutSteam, useSteamId } from "./features/auth/api";
import { useInventory, useSyncInventory } from "./features/backpack/api";
import { BackpackGrid } from "./features/backpack/BackpackGrid";
import { ContextMenu } from "./features/backpack/ContextMenu";
import { StatsBar } from "./features/backpack/StatsBar";
import { FlipFinder } from "./features/flip-finder/FlipFinder";
import { LiveFeed } from "./features/live-feed/LiveFeed";
import { MarketAnalyzer } from "./features/market-analyzer/MarketAnalyzer";
import { Plugins } from "./features/plugins/Plugins";
import { Portfolio } from "./features/portfolio/Portfolio";
import { Simulator } from "./features/simulator/Simulator";
import { TradeAnalyzer } from "./features/trade-analyzer/TradeAnalyzer";
import { TradeHistory } from "./features/trade-history/TradeHistory";

type WorkspaceTab =
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

function App() {
  const { data: steamId, isLoading: steamIdLoading } = useSteamId();

  if (steamIdLoading) {
    return <CenteredMessage>Loading…</CenteredMessage>;
  }

  if (!steamId) {
    return <LoginScreen />;
  }

  return <Workspace steamId={steamId} />;
}

function LoginScreen() {
  const login = useLoginWithSteam();
  return (
    <CenteredMessage>
      <h1 className="mb-4 text-xl font-semibold">TF2 Terminal</h1>
      <button
        type="button"
        onClick={() => login.mutate()}
        disabled={login.isPending}
        className="rounded-md bg-quality-unique px-4 py-2 font-medium text-black hover:opacity-90 disabled:opacity-50"
      >
        {login.isPending ? "Waiting for Steam login…" : "Login with Steam"}
      </button>
      {login.isError && <p className="mt-3 text-sm text-red-400">{login.error.message}</p>}
    </CenteredMessage>
  );
}

function Workspace({ steamId }: { steamId: string }) {
  const [tab, setTab] = useState<WorkspaceTab>("backpack");
  const { data: items = [], isLoading, error } = useInventory();
  const sync = useSyncInventory();
  const logout = useLogoutSteam();

  return (
    <div className="flex h-screen w-screen flex-col bg-charcoal text-zinc-200">
      <div className="flex items-center justify-between border-b border-charcoal-border px-4 py-2 text-sm">
        <div className="flex items-center gap-4">
          <span className="text-zinc-400">Steam: {steamId}</span>
          <nav className="flex gap-1">
            <TabButton active={tab === "backpack"} onClick={() => setTab("backpack")}>
              Backpack
            </TabButton>
            <TabButton active={tab === "market-analyzer"} onClick={() => setTab("market-analyzer")}>
              Market Analyzer
            </TabButton>
            <TabButton active={tab === "trade-analyzer"} onClick={() => setTab("trade-analyzer")}>
              Trade Analyzer
            </TabButton>
            <TabButton active={tab === "live-feed"} onClick={() => setTab("live-feed")}>
              Live Feed
            </TabButton>
            <TabButton active={tab === "alerts"} onClick={() => setTab("alerts")}>
              Alerts
            </TabButton>
            <TabButton active={tab === "flip-finder"} onClick={() => setTab("flip-finder")}>
              Flip Finder
            </TabButton>
            <TabButton active={tab === "portfolio"} onClick={() => setTab("portfolio")}>
              Portfolio
            </TabButton>
            <TabButton active={tab === "trade-history"} onClick={() => setTab("trade-history")}>
              Trade History
            </TabButton>
            <TabButton active={tab === "simulator"} onClick={() => setTab("simulator")}>
              Simulator
            </TabButton>
            <TabButton active={tab === "plugins"} onClick={() => setTab("plugins")}>
              Plugins
            </TabButton>
          </nav>
        </div>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={() => sync.mutate()}
            disabled={sync.isPending}
            className="rounded bg-charcoal-raised px-3 py-1 hover:bg-charcoal-border disabled:opacity-50"
          >
            {sync.isPending ? "Syncing…" : "Sync Inventory"}
          </button>
          <button
            type="button"
            onClick={() => logout.mutate()}
            className="rounded bg-charcoal-raised px-3 py-1 hover:bg-charcoal-border"
          >
            Logout
          </button>
        </div>
      </div>

      {sync.isError && <p className="px-4 py-1 text-sm text-red-400">{sync.error.message}</p>}
      {error && <p className="px-4 py-1 text-sm text-red-400">{error.message}</p>}

      {tab === "backpack" ? (
        <>
          <StatsBar items={items} />
          <div className="min-h-0 flex-1">
            {isLoading ? (
              <CenteredMessage>Loading backpack…</CenteredMessage>
            ) : items.length === 0 ? (
              <CenteredMessage>No items synced yet. Click "Sync Inventory" to fetch your backpack.</CenteredMessage>
            ) : (
              <BackpackGrid items={items} />
            )}
          </div>
          <ContextMenu items={items} />
        </>
      ) : tab === "market-analyzer" ? (
        <MarketAnalyzer />
      ) : tab === "trade-analyzer" ? (
        <TradeAnalyzer />
      ) : tab === "live-feed" ? (
        <LiveFeed />
      ) : tab === "alerts" ? (
        <Alerts />
      ) : tab === "flip-finder" ? (
        <FlipFinder />
      ) : tab === "portfolio" ? (
        <Portfolio />
      ) : tab === "trade-history" ? (
        <TradeHistory />
      ) : tab === "simulator" ? (
        <Simulator />
      ) : (
        <Plugins />
      )}
    </div>
  );
}

function TabButton({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`rounded px-3 py-1 ${active ? "bg-charcoal-raised text-zinc-100" : "text-zinc-400 hover:text-zinc-200"}`}
    >
      {children}
    </button>
  );
}

function CenteredMessage({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-screen w-screen flex-col items-center justify-center bg-charcoal text-center text-zinc-300">
      {children}
    </div>
  );
}

export default App;
