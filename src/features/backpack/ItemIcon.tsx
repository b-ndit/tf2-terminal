import { useState } from "react";

interface ItemIconProps {
  imageUrl: string | null;
  alt: string;
  /** Rendered inside the placeholder when no icon is available/loads —
   * pass this in contexts (tiles) where the icon is the only identifier;
   * omit it where the name is already shown alongside (list rows), since
   * showing it twice is just noise there. */
  fallbackLabel?: string;
  /** Tailwind size classes, e.g. `"h-11 w-11"` — applies to both the
   * image and the placeholder so callers get a fixed-size box either way. */
  size?: string;
}

const DEFAULT_SIZE = "h-11 w-11";

/**
 * Renders an item's icon, falling back to a visibly-a-placeholder box
 * (not bare text) when `imageUrl` is missing or fails to load — most
 * often because the item schema has never been synced (Settings →
 * "Sync Item Schema"), which is what actually populates icon URLs.
 */
export function ItemIcon({ imageUrl, alt, fallbackLabel, size = DEFAULT_SIZE }: ItemIconProps) {
  const [imageFailed, setImageFailed] = useState(false);
  const showImage = Boolean(imageUrl) && !imageFailed;

  if (showImage) {
    return (
      <img
        src={imageUrl ?? undefined}
        alt={alt}
        loading="lazy"
        draggable={false}
        className={`${size} object-contain`}
        onError={() => setImageFailed(true)}
      />
    );
  }

  return (
    <span
      className={`flex ${size} flex-col items-center justify-center gap-0.5 rounded border border-dashed border-charcoal-border p-0.5 text-fg-subtle`}
      title="No icon available — sync the item schema in Settings to fetch icons"
    >
      <span aria-hidden className="text-xs leading-none opacity-50">
        ?
      </span>
      {fallbackLabel && (
        <span className="line-clamp-2 text-center text-[9px] leading-tight text-fg">{fallbackLabel}</span>
      )}
    </span>
  );
}
