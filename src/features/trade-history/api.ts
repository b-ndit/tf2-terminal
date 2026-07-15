import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type TradeLedgerView } from "../../lib/bindings";

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

const LIST_LIMIT = 100;

export function useTrades() {
  return useQuery({
    queryKey: ["trades"],
    queryFn: () => unwrap(commands.listTrades(LIST_LIMIT)),
  });
}

export function useSyncCompletedTrades() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.syncCompletedTrades()),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["trades"] }),
  });
}

export function useSetTradeRating() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: { tradeOfferId: string; rating: number | null }) =>
      unwrap(commands.setTradeRating(input.tradeOfferId, input.rating)),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["trades"] }),
  });
}

export function useSetTradeNotes() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: { tradeOfferId: string; notes: string | null }) =>
      unwrap(commands.setTradeNotes(input.tradeOfferId, input.notes)),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["trades"] }),
  });
}

export type { TradeLedgerView };
