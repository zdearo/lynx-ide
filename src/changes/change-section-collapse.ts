import { create } from "zustand";

import type { ChangeSectionId } from "@/changes/change-sections";

interface CollapsedSectionsState {
  /** The sections the user folded. Kept while the app runs, so switching tabs keeps them shut. */
  collapsed: Partial<Record<ChangeSectionId, boolean>>;
  toggle: (id: ChangeSectionId) => void;
}

export const useCollapsedChangeSections = create<CollapsedSectionsState>(
  (set) => ({
    collapsed: {},
    toggle(id) {
      set((state) => ({
        collapsed: { ...state.collapsed, [id]: !state.collapsed[id] },
      }));
    },
  }),
);
