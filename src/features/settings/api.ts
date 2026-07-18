import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "../../lib/bindings";

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

export type SecretKind = "steamApiKey" | "backpackTfToken" | "discordWebhookUrl";

const HAS: Record<SecretKind, () => ReturnType<typeof commands.hasSteamApiKey>> = {
  steamApiKey: commands.hasSteamApiKey,
  backpackTfToken: commands.hasBackpackTfToken,
  discordWebhookUrl: commands.hasDiscordWebhookUrl,
};
const SET: Record<SecretKind, (value: string) => ReturnType<typeof commands.setSteamApiKey>> = {
  steamApiKey: commands.setSteamApiKey,
  backpackTfToken: commands.setBackpackTfToken,
  discordWebhookUrl: commands.setDiscordWebhookUrl,
};
const CLEAR: Record<SecretKind, () => ReturnType<typeof commands.clearSteamApiKey>> = {
  steamApiKey: commands.clearSteamApiKey,
  backpackTfToken: commands.clearBackpackTfToken,
  discordWebhookUrl: commands.clearDiscordWebhookUrl,
};

export function useHasSecret(kind: SecretKind) {
  return useQuery({
    queryKey: ["settings", "has", kind],
    queryFn: () => unwrap(HAS[kind]()),
  });
}

export function useSetSecret(kind: SecretKind) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (value: string) => unwrap(SET[kind](value)),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["settings", "has", kind] }),
  });
}

export function useClearSecret(kind: SecretKind) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(CLEAR[kind]()),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["settings", "has", kind] }),
  });
}

/** Item names/images come from Valve's schema, not the inventory sync
 * itself — without this, every item shows as "Unknown Item {defindex}"
 * with no icon. */
export function useSyncItemSchema() {
  return useMutation({
    mutationFn: () => unwrap(commands.syncItemSchema()),
  });
}
