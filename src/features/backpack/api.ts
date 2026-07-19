import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type BackpackItem, type Tag } from "../../lib/bindings";

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

export const backpackKeys = {
  inventory: ["inventory"] as const,
  tags: ["tags"] as const,
};

export function useInventory() {
  return useQuery({
    queryKey: backpackKeys.inventory,
    queryFn: () => unwrap(commands.getInventory()),
  });
}

export function useSyncInventory() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.syncInventory()),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: backpackKeys.inventory });
    },
  });
}

export function useTags() {
  return useQuery({
    queryKey: backpackKeys.tags,
    queryFn: () => unwrap(commands.listTags()),
  });
}

function useInvalidateInventory() {
  const queryClient = useQueryClient();
  return () => queryClient.invalidateQueries({ queryKey: backpackKeys.inventory });
}

export function useSetFavorite() {
  const invalidate = useInvalidateInventory();
  return useMutation({
    mutationFn: ({ assetId, favorite }: { assetId: string; favorite: boolean }) =>
      unwrap(commands.setFavorite(assetId, favorite)),
    onSuccess: invalidate,
  });
}

export function useSetPinned() {
  const invalidate = useInvalidateInventory();
  return useMutation({
    mutationFn: ({ assetId, pinned }: { assetId: string; pinned: boolean }) =>
      unwrap(commands.setPinned(assetId, pinned)),
    onSuccess: invalidate,
  });
}

export function useSetFolder() {
  const invalidate = useInvalidateInventory();
  return useMutation({
    mutationFn: ({ assetId, folder }: { assetId: string; folder: string | null }) =>
      unwrap(commands.setFolder(assetId, folder)),
    onSuccess: invalidate,
  });
}

export function useSetNote() {
  const invalidate = useInvalidateInventory();
  return useMutation({
    mutationFn: ({ assetId, note }: { assetId: string; note: string | null }) =>
      unwrap(commands.setNote(assetId, note)),
    onSuccess: invalidate,
  });
}

export function useSetCustomLabel() {
  const invalidate = useInvalidateInventory();
  return useMutation({
    mutationFn: ({ assetId, label }: { assetId: string; label: string | null }) =>
      unwrap(commands.setCustomLabel(assetId, label)),
    onSuccess: invalidate,
  });
}

export function useCreateTag() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ name, color }: { name: string; color: string }) =>
      unwrap(commands.createTag(name, color)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: backpackKeys.tags });
    },
  });
}

export function useAddItemTag() {
  const invalidate = useInvalidateInventory();
  return useMutation({
    mutationFn: ({ assetId, tagId }: { assetId: string; tagId: number }) =>
      unwrap(commands.addItemTag(assetId, tagId)),
    onSuccess: invalidate,
  });
}

export function useRemoveItemTag() {
  const invalidate = useInvalidateInventory();
  return useMutation({
    mutationFn: ({ assetId, tagId }: { assetId: string; tagId: number }) =>
      unwrap(commands.removeItemTag(assetId, tagId)),
    onSuccess: invalidate,
  });
}

export type { BackpackItem, Tag };
