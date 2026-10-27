/** Puts keyboard focus on a member's card on the map, or its row in the tree. */
export function focusMemberCard(id: string | null) {
  if (id) {
    document.querySelector<HTMLElement>(`[data-team-id="${id}"]`)?.focus();
  }
}
