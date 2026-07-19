import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type AnalyzedTradeOffer, type PartnerItemView, type TradeItemView } from "../../lib/bindings";

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

// Backend recomputes verdicts fresh on every call (no push/event path —
// see the Module 9 note in `services::trade_analysis_engine`), so the
// panel polls on an interval instead.
const POLL_INTERVAL_MS = 20_000;

export function useActiveTrades() {
  return useQuery({
    queryKey: ["active-trades"],
    queryFn: () => unwrap(commands.getActiveTrades()),
    refetchInterval: POLL_INTERVAL_MS,
  });
}

function useInvalidateActiveTrades() {
  const queryClient = useQueryClient();
  return () => queryClient.invalidateQueries({ queryKey: ["active-trades"] });
}

/** Requires a connected Steam session (Settings) — a plain Steam API key
 * only lets `useActiveTrades` read offers, not act on them. */
export function useAcceptTradeOffer() {
  const invalidate = useInvalidateActiveTrades();
  return useMutation({
    mutationFn: ({ tradeOfferId, partnerSteamId }: { tradeOfferId: string; partnerSteamId: string }) =>
      unwrap(commands.acceptTradeOffer(tradeOfferId, partnerSteamId)),
    onSuccess: invalidate,
  });
}

export function useDeclineTradeOffer() {
  const invalidate = useInvalidateActiveTrades();
  return useMutation({
    mutationFn: (tradeOfferId: string) => unwrap(commands.declineTradeOffer(tradeOfferId)),
    onSuccess: invalidate,
  });
}

// A partner SteamID64 is always 17 digits (`765611...`) — gates the query
// so it doesn't fire on every keystroke of a still-incomplete id.
const STEAM_ID64_PATTERN = /^\d{17}$/;

export function usePublicInventory(partnerSteamId: string) {
  const enabled = STEAM_ID64_PATTERN.test(partnerSteamId);
  return useQuery({
    queryKey: ["public-inventory", partnerSteamId],
    queryFn: () => unwrap(commands.getPublicInventory(partnerSteamId)),
    enabled,
  });
}

export function useSendTradeOffer() {
  const invalidate = useInvalidateActiveTrades();
  return useMutation({
    mutationFn: ({
      partnerSteamId,
      myAssetIds,
      theirAssetIds,
      message,
    }: {
      partnerSteamId: string;
      myAssetIds: string[];
      theirAssetIds: string[];
      message: string;
    }) => unwrap(commands.sendTradeOffer(partnerSteamId, myAssetIds, theirAssetIds, message)),
    onSuccess: invalidate,
  });
}

export type { AnalyzedTradeOffer, PartnerItemView, TradeItemView };
