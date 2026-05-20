export const LANGUAGE_VALIDATION_MESSAGE =
  "Language must be a valid BCP-47 locale (for example: en-US, fr-FR, zh-Hant-TW).";

function isAlpha(value: string): boolean {
  return /^[A-Za-z]+$/.test(value);
}

function isNumeric(value: string): boolean {
  return /^[0-9]+$/.test(value);
}

function isAlphanumeric(value: string): boolean {
  return /^[A-Za-z0-9]+$/.test(value);
}

function toTitleCase(value: string): string {
  if (!value) {
    return '';
  }
  return value[0].toUpperCase() + value.slice(1).toLowerCase();
}

function isVariantSubtag(value: string): boolean {
  if (!isAlphanumeric(value)) {
    return false;
  }

  if (value.length >= 5 && value.length <= 8) {
    return true;
  }

  return value.length === 4 && /^[0-9]/.test(value);
}

export function normalizeLocaleTag(value: string): string | null {
  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }

  let normalized = trimmed.replace(/_/g, '-').toLowerCase();
  if (
    normalized.startsWith('-')
    || normalized.endsWith('-')
    || normalized.includes('--')
  ) {
    return null;
  }

  if (/^[a-z]{4}$/.test(normalized)) {
    normalized = `${normalized.slice(0, 2)}-${normalized.slice(2)}`;
  }

  const parts = normalized.split('-');
  const primary = parts[0];
  if (!primary || !isAlpha(primary) || primary.length < 2 || primary.length > 3) {
    return null;
  }

  let script: string | null = null;
  let region: string | null = null;
  const variants: string[] = [];

  for (let i = 1; i < parts.length; i += 1) {
    const part = parts[i];
    if (!part) {
      return null;
    }

    if (!script && !region && part.length === 4 && isAlpha(part)) {
      script = toTitleCase(part);
      continue;
    }

    if (!region && ((part.length === 2 && isAlpha(part)) || (part.length === 3 && isNumeric(part)))) {
      region = part.length === 2 ? part.toUpperCase() : part;
      continue;
    }

    if (isVariantSubtag(part)) {
      variants.push(part.toLowerCase());
      continue;
    }

    return null;
  }

  const canonical: string[] = [primary.toLowerCase()];
  if (script) {
    canonical.push(script);
  }
  if (region) {
    canonical.push(region);
  }
  canonical.push(...variants);

  return canonical.join('-');
}

export function validateLocaleTag(value: string): string | null {
  return normalizeLocaleTag(value) ? null : LANGUAGE_VALIDATION_MESSAGE;
}
