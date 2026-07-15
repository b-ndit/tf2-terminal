import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type PluginSummary } from "../../lib/bindings";

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

const pluginsKey = ["plugins"] as const;

export function useListPlugins() {
  return useQuery({
    queryKey: pluginsKey,
    queryFn: () => unwrap(commands.listPlugins()),
  });
}

export function useInstallPlugin() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (sourceDir: string) => unwrap(commands.installPlugin(sourceDir)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: pluginsKey });
    },
  });
}

export function useSetPluginEnabled() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ name, enabled }: { name: string; enabled: boolean }) =>
      unwrap(commands.setPluginEnabled(name, enabled)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: pluginsKey });
    },
  });
}

export function useUninstallPlugin() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (name: string) => unwrap(commands.uninstallPlugin(name)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: pluginsKey });
    },
  });
}

export function usePluginPanelPath(name: string, enabled: boolean) {
  return useQuery({
    queryKey: ["plugin-panel", name],
    queryFn: () => unwrap(commands.getPluginPanelPath(name)),
    enabled,
  });
}

export type { PluginSummary };
