import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, events, type AlertEventView, type AlertFired, type AlertRuleView } from "../../lib/bindings";

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

const RECENT_EVENTS_LIMIT = 50;

export function useAlertRules() {
  return useQuery({
    queryKey: ["alert-rules"],
    queryFn: () => unwrap(commands.listAlertRules()),
  });
}

export function useCreateAlertRule() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: { url: string; kind: string; threshold: number | null; channels: string[] }) =>
      unwrap(commands.createAlertRule(input.url, input.kind, input.threshold, input.channels)),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["alert-rules"] }),
  });
}

export function useSetAlertRuleEnabled() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: { ruleId: number; enabled: boolean }) =>
      unwrap(commands.setAlertRuleEnabled(input.ruleId, input.enabled)),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["alert-rules"] }),
  });
}

export function useDeleteAlertRule() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (ruleId: number) => unwrap(commands.deleteAlertRule(ruleId)),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["alert-rules"] }),
  });
}

/// Seeds from `list_recent_alert_events`, then prepends live-fired alerts
/// pushed by `services::alert_service`. Also plays a short Web Audio beep
/// when a live alert's channels include "sound" — the client-side half of
/// the sound-sink deviation (`docs/DESIGN.md` §6's Module 10 note): the
/// backend only tracks "sound" as a channel value, it never plays audio
/// itself.
export function useAlertEvents() {
  const seed = useQuery({
    queryKey: ["alert-events"],
    queryFn: () => unwrap(commands.listRecentAlertEvents(RECENT_EVENTS_LIMIT)),
  });
  const [live, setLive] = useState<AlertFired[]>([]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    events.alertFired.listen((event) => {
      const fired = event.payload;
      setLive((prev) => [fired, ...prev].slice(0, RECENT_EVENTS_LIMIT));
      if (fired.channels.includes("sound")) {
        playBeep();
      }
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

  const recent = [...live, ...(seed.data ?? [])].slice(0, RECENT_EVENTS_LIMIT);

  return { recent, isLoading: seed.isLoading, error: seed.error };
}

function playBeep() {
  try {
    const AudioContextClass = window.AudioContext ?? (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    const ctx = new AudioContextClass();
    const oscillator = ctx.createOscillator();
    const gain = ctx.createGain();
    oscillator.type = "sine";
    oscillator.frequency.value = 880;
    gain.gain.value = 0.15;
    oscillator.connect(gain);
    gain.connect(ctx.destination);
    oscillator.start();
    oscillator.stop(ctx.currentTime + 0.2);
    oscillator.onended = () => ctx.close();
  } catch {
    // Web Audio unavailable in this environment — sound is a nice-to-have
    // channel, not a critical one; the desktop/discord sinks still fired.
  }
}

export type { AlertEventView, AlertFired, AlertRuleView };
