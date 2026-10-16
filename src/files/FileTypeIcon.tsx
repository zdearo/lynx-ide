import {
  Binary,
  BookOpen,
  Container,
  Database,
  File,
  FileArchive,
  FileAudio,
  FileCode,
  FileCog,
  FileImage,
  FileJson,
  FileKey,
  FileLock,
  FileSpreadsheet,
  FileTerminal,
  FileText,
  FileType,
  FileVideo,
  Folder,
  FolderCog,
  FolderGit2,
  FolderOpen,
  GitBranch,
  Hammer,
  Package,
  Scale,
  type LucideIcon,
} from "lucide-react";

import { fileIconKind, type FileIconKind } from "@/files/file-icon-kind";
import { cn } from "@/lib/utils";

const ICONS: Record<FileIconKind, LucideIcon> = {
  folder: Folder,
  "folder-open": FolderOpen,
  "folder-git": FolderGit2,
  "folder-config": FolderCog,
  code: FileCode,
  terminal: FileTerminal,
  data: FileJson,
  config: FileCog,
  text: FileText,
  readme: BookOpen,
  image: FileImage,
  archive: FileArchive,
  audio: FileAudio,
  video: FileVideo,
  font: FileType,
  spreadsheet: FileSpreadsheet,
  database: Database,
  binary: Binary,
  package: Package,
  docker: Container,
  build: Hammer,
  license: Scale,
  git: GitBranch,
  secret: FileKey,
  lockfile: FileLock,
  file: File,
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
