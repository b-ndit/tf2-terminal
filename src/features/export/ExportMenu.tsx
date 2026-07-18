import { useEffect, useRef, useState } from "react";
import type { ExportFormat } from "../../lib/bindings";
import { useExport, type ExportDataset } from "./api";

const FORMATS: ExportFormat[] = ["csv", "xlsx", "json", "pdf"];

export function ExportMenu({ dataset }: { dataset: ExportDataset }) {
  const [open, setOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const exportMutation = useExport(dataset);

  useEffect(() => {
    if (!open) return;
    function onPointerDown(e: PointerEvent) {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [open]);

  return (
    <div ref={menuRef} className="relative inline-block">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        disabled={exportMutation.isPending}
        className="rounded bg-charcoal-raised px-3 py-1 text-sm hover:bg-charcoal-border disabled:opacity-50"
      >
        {exportMutation.isPending ? "Exporting…" : "Export ▾"}
      </button>
      {open && (
        <div className="absolute right-0 z-10 mt-1 flex w-28 flex-col rounded border border-charcoal-border bg-charcoal-raised shadow-xl">
          {FORMATS.map((format) => (
            <button
              key={format}
              type="button"
              onClick={() => {
                setOpen(false);
                exportMutation.mutate(format);
              }}
              className="px-3 py-1.5 text-left text-sm uppercase text-fg-muted hover:bg-charcoal-border hover:text-fg"
            >
              {format}
            </button>
          ))}
        </div>
      )}
      {exportMutation.isError && (
        <p className="absolute right-0 z-10 mt-1 w-max max-w-xs rounded bg-charcoal-raised px-2 py-1 text-xs text-red-400 shadow-xl">
          {exportMutation.error.message}
        </p>
      )}
    </div>
  );
}
