import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type FlipOpportunityView, type WatchlistItemView } from "../../lib/bindings";

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

// Backend recomputes the scan fresh on every call (no persistence, no
// push — same "pull, like Market Analyzer/Trade Analyzer" shape), so the
// panel polls on an interval instead.
const POLL_INTERVAL_MS = 30_000;

export function useFlipOpportunities(minRoiPct: number | null, minConfidence: number | null) {
  return useQuery({
    queryKey: ["flip-opportunities", minRoiPct, minConfidence],
    queryFn: () => unwrap(commands.getFlipOpportunities(minRoiPct, minConfidence)),
    refetchInterval: POLL_INTERVAL_MS,
  });
}

export function useWatchlist() {
  return useQuery({
    queryKey: ["watchlist"],
    queryFn: () => unwrap(commands.listWatchlist()),
  });
}

export function useAddToWatchlist() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (url: string) => unwrap(commands.addToWatchlist(url)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["watchlist"] });
      queryClient.invalidateQueries({ queryKey: ["flip-opportunities"] });
    },
  });
}

export function useRemoveFromWatchlist() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (itemId: number) => unwrap(commands.removeFromWatchlist(itemId)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["watchlist"] });
      queryClient.invalidateQueries({ queryKey: ["flip-opportunities"] });
    },
  });
}

export type { FlipOpportunityView, WatchlistItemView };
