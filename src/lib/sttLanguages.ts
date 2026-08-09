/** Curated STT languages shown in Settings (top ~20 + required locales). */
export type SttLanguage = {
  code: string;
  label: string;
  /** Included in Deepgram Nova-3 `language=multi` code-switching. */
  multi: boolean;
};

export const STT_LANGUAGES: SttLanguage[] = [
  { code: "en", label: "English", multi: true },
  { code: "es", label: "Spanish", multi: true },
  { code: "zh", label: "Chinese (Mandarin)", multi: false },
  { code: "hi", label: "Hindi", multi: true },
  { code: "ar", label: "Arabic", multi: false },
  { code: "fr", label: "French", multi: true },
  { code: "pt", label: "Portuguese", multi: true },
  { code: "ru", label: "Russian", multi: true },
  { code: "de", label: "German", multi: true },
  { code: "ja", label: "Japanese", multi: true },
  { code: "ko", label: "Korean", multi: false },
  { code: "it", label: "Italian", multi: true },
  { code: "tr", label: "Turkish", multi: false },
  { code: "vi", label: "Vietnamese", multi: false },
  { code: "pl", label: "Polish", multi: false },
  { code: "uk", label: "Ukrainian", multi: false },
  { code: "nl", label: "Dutch", multi: true },
  { code: "id", label: "Indonesian", multi: false },
  { code: "th", label: "Thai", multi: false },
  { code: "bg", label: "Bulgarian", multi: false },
];

export const MAX_STT_LANGUAGES = 5;

export const DEFAULT_STT_LANGUAGES = ["en"];

/** When multilingual is off, keep a single language — prefer English if present. */
export function monolingualLanguages(codes: string[]): string[] {
  const unique = [...new Set(codes.map((c) => c.trim().toLowerCase()).filter(Boolean))];
  if (unique.length === 0) return [...DEFAULT_STT_LANGUAGES];
  if (unique.length === 1) return unique;
  if (unique.includes("en")) return ["en"];
  return [unique[0]];
}

export function parseSttLanguages(raw: string | null | undefined): string[] {
  if (!raw || !raw.trim()) return [...DEFAULT_STT_LANGUAGES];
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (Array.isArray(parsed)) {
      const allowed = new Set(STT_LANGUAGES.map((l) => l.code));
      const codes = parsed
        .filter((c): c is string => typeof c === "string")
        .map((c) => c.trim().toLowerCase())
        .filter((c) => allowed.has(c));
      const unique = [...new Set(codes)].slice(0, MAX_STT_LANGUAGES);
      return unique.length > 0 ? unique : [...DEFAULT_STT_LANGUAGES];
    }
  } catch {
    // fall through — comma-separated
  }
  const allowed = new Set(STT_LANGUAGES.map((l) => l.code));
  const codes = raw
    .split(/[,+\s]+/)
    .map((c) => c.trim().toLowerCase())
    .filter((c) => allowed.has(c));
  const unique = [...new Set(codes)].slice(0, MAX_STT_LANGUAGES);
  return unique.length > 0 ? unique : [...DEFAULT_STT_LANGUAGES];
}
