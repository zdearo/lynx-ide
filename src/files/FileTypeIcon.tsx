import {
  BinaryIcon,
  BookOpenIcon,
  BracketsCurlyIcon,
  CodeIcon,
  CubeIcon,
  DatabaseIcon,
  FileAudioIcon,
  FileIcon,
  FileImageIcon,
  FileIniIcon,
  FileLockIcon,
  FileTextIcon,
  FileVideoIcon,
  FileZipIcon,
  FolderIcon,
  FolderOpenIcon,
  FolderSimpleDashedIcon,
  FolderSimpleIcon,
  GitBranchIcon,
  HammerIcon,
  KeyIcon,
  PackageIcon,
  ScalesIcon,
  TableIcon,
  TerminalIcon,
  TextAaIcon,
} from "@phosphor-icons/react";
import type { IconComponent } from "@/components/icons/icon-component";

import { fileIconKind, type FileIconKind } from "@/files/file-icon-kind";
import { cn } from "@/lib/utils";

const ICONS: Record<FileIconKind, IconComponent> = {
  folder: FolderIcon,
  "folder-open": FolderOpenIcon,
  "folder-git": FolderSimpleIcon,
  "folder-config": FolderSimpleDashedIcon,
  code: CodeIcon,
  terminal: TerminalIcon,
  data: BracketsCurlyIcon,
  config: FileIniIcon,
  text: FileTextIcon,
  readme: BookOpenIcon,
  image: FileImageIcon,
  archive: FileZipIcon,
  audio: FileAudioIcon,
  video: FileVideoIcon,
  font: TextAaIcon,
  spreadsheet: TableIcon,
  database: DatabaseIcon,
  binary: BinaryIcon,
  package: PackageIcon,
  docker: CubeIcon,
  build: HammerIcon,
  license: ScalesIcon,
  git: GitBranchIcon,
  secret: KeyIcon,
  lockfile: FileLockIcon,
  file: FileIcon,
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
