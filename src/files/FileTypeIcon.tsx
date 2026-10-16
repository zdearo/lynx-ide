import { BalanceGlyph18 } from "@/components/icons/BalanceGlyph18";
import { BinaryGlyph18 } from "@/components/icons/BinaryGlyph18";
import { BookOpenGlyph18 } from "@/components/icons/BookOpenGlyph18";
import { Box2Glyph18 } from "@/components/icons/Box2Glyph18";
import { BoxGlyph18 } from "@/components/icons/BoxGlyph18";
import { BracketsCurlyGlyph18 } from "@/components/icons/BracketsCurlyGlyph18";
import { CodeBranchGlyph18 } from "@/components/icons/CodeBranchGlyph18";
import { CodeGlyph18 } from "@/components/icons/CodeGlyph18";
import { DatabaseGlyph18 } from "@/components/icons/DatabaseGlyph18";
import { FileGlyph18 } from "@/components/icons/FileGlyph18";
import { FileImageGlyph18 } from "@/components/icons/FileImageGlyph18";
import { FileKeyGlyph18 } from "@/components/icons/FileKeyGlyph18";
import { FileLockGlyph18 } from "@/components/icons/FileLockGlyph18";
import { FileMusicGlyph18 } from "@/components/icons/FileMusicGlyph18";
import { FileSettingsGlyph18 } from "@/components/icons/FileSettingsGlyph18";
import { FileZipGlyph18 } from "@/components/icons/FileZipGlyph18";
import { Folder2Glyph18 } from "@/components/icons/Folder2Glyph18";
import { FolderGlyph18 } from "@/components/icons/FolderGlyph18";
import { FolderOpenGlyph18 } from "@/components/icons/FolderOpenGlyph18";
import { FolderSettingsGlyph18 } from "@/components/icons/FolderSettingsGlyph18";
import { HammerGlyph18 } from "@/components/icons/HammerGlyph18";
import type { IconComponent } from "@/components/icons/icon-component";
import { PageGlyph18 } from "@/components/icons/PageGlyph18";
import { SquareTableGlyph18 } from "@/components/icons/SquareTableGlyph18";
import { TerminalGlyph18 } from "@/components/icons/TerminalGlyph18";
import { TypographyGlyph18 } from "@/components/icons/TypographyGlyph18";
import { VideoGlyph18 } from "@/components/icons/VideoGlyph18";

import { fileIconKind, type FileIconKind } from "@/files/file-icon-kind";
import { cn } from "@/lib/utils";

const ICONS: Record<FileIconKind, IconComponent> = {
  folder: FolderGlyph18,
  "folder-open": FolderOpenGlyph18,
  "folder-git": Folder2Glyph18,
  "folder-config": FolderSettingsGlyph18,
  code: CodeGlyph18,
  terminal: TerminalGlyph18,
  data: BracketsCurlyGlyph18,
  config: FileSettingsGlyph18,
  text: PageGlyph18,
  readme: BookOpenGlyph18,
  image: FileImageGlyph18,
  archive: FileZipGlyph18,
  audio: FileMusicGlyph18,
  video: VideoGlyph18,
  font: TypographyGlyph18,
  spreadsheet: SquareTableGlyph18,
  database: DatabaseGlyph18,
  binary: BinaryGlyph18,
  package: Box2Glyph18,
  docker: BoxGlyph18,
  build: HammerGlyph18,
  license: BalanceGlyph18,
  git: CodeBranchGlyph18,
  secret: FileKeyGlyph18,
  lockfile: FileLockGlyph18,
  file: FileGlyph18,
};

export function FileTypeIcon({
  name,
  isDirectory,
  open,
  className,
}: {
  name: string;
  isDirectory: boolean;
  open: boolean;
  className?: string;
}) {
  const Icon = ICONS[fileIconKind(name, isDirectory, open)];
  return <Icon aria-hidden className={cn("size-3.5 shrink-0", className)} />;
}
