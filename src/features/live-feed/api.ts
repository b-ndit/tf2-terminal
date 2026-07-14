import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { commands, events, type ListingEvent } from "../../lib/bindings";

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

const MAX_FEED_LENGTH = 200;

/// Seeds from the existing bounded ring buffer (`get_recent_listings`,
/// Module 5/7 — unused by any UI until now), then prepends live arrivals
/// pushed by `services::live_feed`'s relay. Newest first.
export function useLiveFeed() {
  const seed = useQuery({
    queryKey: ["recent-listings"],
    queryFn: () => unwrap(commands.getRecentListings()),
  });
  const [live, setLive] = useState<ListingEvent[]>([]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    events.listingEventPushed.listen((event) => {
      setLive((prev) => [event.payload, ...prev].slice(0, MAX_FEED_LENGTH));
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const seeded = seed.data ? [...seed.data].reverse() : [];
  const feed = [...live, ...seeded].slice(0, MAX_FEED_LENGTH);

  return { feed, isLoading: seed.isLoading, error: seed.error };
}

export type { ListingEvent };
