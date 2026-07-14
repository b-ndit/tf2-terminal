import { useMutation, useQuery } from "@tanstack/react-query";
import { commands, type ItemAnalytics, type PriceBar } from "../../lib/bindings";

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

export function useAnalyzeClassifiedUrl() {
  return useMutation({
    mutationFn: (url: string) => unwrap(commands.analyzeClassifiedUrl(url)),
  });
}

// `url` is `null` until an analysis has succeeded — the chart has nothing
// to resolve against before then.
export function usePriceHistory(url: string | null) {
  return useQuery({
    queryKey: ["price-history", url],
    queryFn: () => unwrap(commands.getPriceHistory(url as string)),
    enabled: url !== null,
  });
}

export type { ItemAnalytics, PriceBar };
