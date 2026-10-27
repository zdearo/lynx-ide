import { copy } from "@/ui-copy";

/** An image of the worktree, centered and scaled down to fit; never stretched past its size. */
export function ImageView({ name, url }: { name: string; url: string }) {
  return (
    <div className="flex h-full min-h-0 items-center justify-center overflow-auto bg-muted/30 p-4">
      <img
        src={url}
        alt={copy.editor.imageLabel(name)}
        className="max-h-full max-w-full object-contain"
      />
    </div>
  );
}
