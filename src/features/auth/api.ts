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

const steamIdKey = ["auth", "steamId"] as const;

export function useSteamId() {
  return useQuery({
    queryKey: steamIdKey,
    queryFn: () => unwrap(commands.getSteamId()),
  });
}

export function useLoginWithSteam() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.loginWithSteam()),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: steamIdKey }),
  });
}

export function useLogoutSteam() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.logoutSteam()),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: steamIdKey }),
  });
}
