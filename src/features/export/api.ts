import { useMutation } from "@tanstack/react-query";
import { save } from "@tauri-apps/plugin-dialog";
import { commands, type ExportFormat } from "../../lib/bindings";

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

export type ExportDataset = "backpack" | "trade-history" | "portfolio";

const EXPORT_COMMANDS: Record<ExportDataset, typeof commands.exportBackpack> = {
  backpack: commands.exportBackpack,
  "trade-history": commands.exportTradeHistory,
  portfolio: commands.exportPortfolio,
};

const DEFAULT_FILE_NAMES: Record<ExportDataset, string> = {
  backpack: "backpack",
  "trade-history": "trade-history",
  portfolio: "portfolio",
};

/** Mutation that opens the native save picker, then calls the matching
 * export command with the chosen path — a no-op (not an error) if the
 * user cancels the picker. */
export function useExport(dataset: ExportDataset) {
  return useMutation({
    mutationFn: async (format: ExportFormat) => {
      const path = await save({
        title: `Export ${DEFAULT_FILE_NAMES[dataset]} as ${format.toUpperCase()}`,
        defaultPath: `${DEFAULT_FILE_NAMES[dataset]}.${format}`,
        filters: [{ name: format.toUpperCase(), extensions: [format] }],
      });
      if (!path) return;
      await unwrap(EXPORT_COMMANDS[dataset](format, path));
    },
  });
}

export type { ExportFormat };
