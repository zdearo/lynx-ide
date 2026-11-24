import { isValidElement, type ReactElement, type ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { ContextMenuCheckboxItem } from "@/components/ui/context-menu";
import { TEAM_MEMBERS } from "@/team/team-fixtures";
import { TeamMemberContextMenu } from "@/team/TeamMemberMenu";

const { useTeamMembers } = vi.hoisted(() => ({
  useTeamMembers: vi.fn((): unknown[] => []),
}));
vi.mock("@/team/team-store", () => ({
  openMember: vi.fn(),
  useTeamMembers,
  useTeamStore: vi.fn(),
}));

const builder = TEAM_MEMBERS.find((member) => member.id === "builder");
if (!builder) throw new Error("the mock team has no builder");

// The menus are plain functions of their props, so the element they return can be read without a DOM.
function checkboxOf(element: ReactElement, type: unknown) {
  const children = (element.props as { children: ReactNode[] }).children;
  return children.find(
    (child): child is ReactElement<{ disabled: boolean }> =>
      isValidElement(child) && child.type === type,
  );
}

describe("member menus", () => {
  beforeEach(() => useTeamMembers.mockClear());

  it("takes the has-members flag from its card and never reads the team", () => {
    const menu = TeamMemberContextMenu({ member: builder, hasChildren: true });

    expect(checkboxOf(menu, ContextMenuCheckboxItem)?.props.disabled).toBe(
      true,
    );
    expect(useTeamMembers).not.toHaveBeenCalled();
  });

  it("keeps the orchestrator switch on for a member with no members under it", () => {
    const menu = TeamMemberContextMenu({ member: builder, hasChildren: false });

    expect(checkboxOf(menu, ContextMenuCheckboxItem)?.props.disabled).toBe(
      false,
    );
  });
});
