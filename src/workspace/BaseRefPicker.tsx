import { useMemo, useState } from "react";

import { CheckGlyph18 } from "@/components/icons/CheckGlyph18";
import { ChevronDownGlyph18 } from "@/components/icons/ChevronDownGlyph18";
import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { copy } from "@/ui-copy";
import { searchBaseRefs } from "@/workspace/base-ref-search";

type BaseRefPickerProps = {
  id: string;
  /** The ref the new worktree starts from. */
  value: string;
  /** Every ref to choose from, or null while they load. */
  options: string[] | null;
  onChange: (ref: string) => void;
};

/**
 * Searchable base picker. Only the rows that match are rendered, so a repo
 * with thousands of branches opens as fast as one with a dozen.
 */
export function BaseRefPicker({
  id,
  value,
  options,
  onChange,
}: BaseRefPickerProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const loading = options === null;
  const search = useMemo(
    () => searchBaseRefs(options ?? [], query),
    [options, query],
  );

  // Every path that opens or closes the picker goes through here, so a
  // reopen never starts from the last search, whether a row was picked or not.
  function setPickerOpen(next: boolean) {
    if (next) setQuery("");
    setOpen(next);
  }

  function choose(ref: string) {
    onChange(ref);
    setPickerOpen(false);
  }

  return (
    <Popover open={open} onOpenChange={setPickerOpen}>
      <PopoverTrigger asChild>
        <Button
          id={id}
          variant="outline"
          role="combobox"
          aria-expanded={open}
          disabled={loading}
          className="w-full justify-between font-normal"
        >
          <span className="truncate">
            {loading ? copy.newWorktree.loadingRefs : value}
          </span>
          <ChevronDownGlyph18 className="text-muted-foreground" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="w-(--radix-popover-trigger-width)"
      >
        {/* cmdk would filter every row on each keystroke; the search above already did. */}
        <Command shouldFilter={false}>
          <CommandInput
            value={query}
            onValueChange={setQuery}
            placeholder={copy.newWorktree.searchBranches}
            aria-label={copy.newWorktree.searchBranches}
          />
          <CommandList>
            <CommandEmpty>{copy.newWorktree.noBranches}</CommandEmpty>
            <CommandGroup>
              {search.shown.map((ref) => (
                <CommandItem key={ref} value={ref} onSelect={() => choose(ref)}>
                  <span className="truncate">{ref}</span>
                  {ref === value && (
                    <CheckGlyph18 className="ml-auto shrink-0" />
                  )}
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
          {search.total > search.shown.length && (
            <p className="border-t px-3 py-2 text-xs text-muted-foreground">
              {copy.newWorktree.moreBranches(search.shown.length, search.total)}
            </p>
          )}
        </Command>
      </PopoverContent>
    </Popover>
  );
}
