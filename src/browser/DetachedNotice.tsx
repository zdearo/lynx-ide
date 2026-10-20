import { OpenInNewWindowGlyph18 } from "@/components/icons/OpenInNewWindowGlyph18";
import { Button } from "@/components/ui/button";
import { useBrowserStore } from "@/browser/browser-store";
import { copy } from "@/ui-copy";

/** Shown in the main window while the browser has a window of its own. */
export function DetachedNotice() {
  const attach = useBrowserStore((state) => state.attach);

  return (
    <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-background px-6 text-center">
      <OpenInNewWindowGlyph18 className="text-muted-foreground" />
      <div className="max-w-sm">
        <h2 className="text-sm font-medium">{copy.browser.detachedTitle}</h2>
        <p className="mt-1 text-ui text-muted-foreground">
          {copy.browser.detachedBody}
        </p>
      </div>
      <Button variant="outline" size="sm" onClick={() => void attach()}>
        {copy.browser.bringBack}
      </Button>
    </div>
  );
}
