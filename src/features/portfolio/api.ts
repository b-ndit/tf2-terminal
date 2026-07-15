import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type ItemMoverView, type PlWindowsView, type PortfolioSnapshotView } from "../../lib/bindings";

async function unwrap<T>(promise: Promise<{ status: "ok"; data: T } | { status: "error"; error: unknown }>): Promise<T> {
  const result = await promise;
  if (result.status === "error") {
    const message =
      typeof result.error === "object" && result.error !== null && "message" in result.error
        ? String((result.error as { message: unknown }).message)
        : String(result.error);
    throw new Error(message);
  }
  return result.data;
}

const HISTORY_DAYS = 90;

export function usePortfolioSnapshot() {
  return useQuery({
    queryKey: ["portfolio-snapshot"],
    queryFn: () => unwrap(commands.getPortfolioSnapshot()),
  });
}

export function usePortfolioHistory() {
  return useQuery({
    queryKey: ["portfolio-history"],
    queryFn: () => unwrap(commands.getPortfolioHistory(HISTORY_DAYS)),
  });
}

export function usePlWindows() {
  return useQuery({
    queryKey: ["pl-windows"],
    queryFn: () => unwrap(commands.getPlWindows()),
  });
}

export function useWinnersLosers(windowDays: number) {
  return useQuery({
    queryKey: ["winners-losers", windowDays],
    queryFn: () => unwrap(commands.getWinnersLosers(windowDays)),
  });
}

// A manual "Refresh" covers the gap before the backend's daily periodic
// snapshot fires (`portfolio_service::spawn_periodic_snapshot`) — same
// on-demand command either way, just re-invalidates everything that
// depends on the latest snapshot.
export function useRefreshPortfolio() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.getPortfolioSnapshot()),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["portfolio-snapshot"] });
      queryClient.invalidateQueries({ queryKey: ["portfolio-history"] });
      queryClient.invalidateQueries({ queryKey: ["pl-windows"] });
    },
  });
}

export type { ItemMoverView, PlWindowsView, PortfolioSnapshotView };
