import { useQuery } from "@tanstack/react-query";
import { commands, type AnalyzedTradeOffer } from "../../lib/bindings";

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

export type { AnalyzedTradeOffer };
