import { create } from "zustand";

import type { ProjectFilter } from "@/workspace/projects/project-filter";

interface ProjectFilterState {
  filter: ProjectFilter;
  setFilter: (filter: ProjectFilter) => void;
}

/** Which projects the sidebar lists. In memory only: it starts at All each time the app opens. */
export const useProjectFilterStore = create<ProjectFilterState>((set) => ({
  filter: "all",
  setFilter: (filter) => set({ filter }),
}));
