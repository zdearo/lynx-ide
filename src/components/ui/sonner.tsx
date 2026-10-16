"use client";

import { Toaster as Sonner, type ToasterProps } from "sonner";
import { CircleCheckGlyph18 } from "@/components/icons/CircleCheckGlyph18";
import { CircleInfoGlyph18 } from "@/components/icons/CircleInfoGlyph18";
import { CircleXmarkGlyph18 } from "@/components/icons/CircleXmarkGlyph18";
import { Loader2Glyph18 } from "@/components/icons/Loader2Glyph18";
import { TriangleWarningGlyph18 } from "@/components/icons/TriangleWarningGlyph18";

const Toaster = ({ ...props }: ToasterProps) => {
  return (
    <Sonner
      theme="dark"
      className="toaster group"
      icons={{
        success: <CircleCheckGlyph18 className="size-4" />,
        info: <CircleInfoGlyph18 className="size-4" />,
        warning: <TriangleWarningGlyph18 className="size-4" />,
        error: <CircleXmarkGlyph18 className="size-4" />,
        loading: <Loader2Glyph18 className="size-4 animate-spin" />,
      }}
      style={
        {
          "--normal-bg": "var(--popover)",
          "--normal-text": "var(--popover-foreground)",
          "--normal-border": "var(--border)",
          "--border-radius": "var(--radius)",
        } as React.CSSProperties
      }
      toastOptions={{
        classNames: {
          toast: "cn-toast",
        },
      }}
      {...props}
    />
  );
};

export { Toaster };
