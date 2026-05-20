export type LocalPayloadKind = 'File' | 'Directory';

export interface LocalPayloadItem {
  sourcePath: string;
  sourceKind: LocalPayloadKind;
  displayName?: string;
}

export function deriveLocalPayloadDisplayName(item: LocalPayloadItem): string {
  const explicit = item.displayName?.trim();
  if (explicit) {
    return explicit;
  }

  const parts = item.sourcePath.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] || item.sourcePath;
}
