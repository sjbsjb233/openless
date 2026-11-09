// Qwen-Audio 3.1 ASR options (Alibaba Bailian), stored under the `qwenAsr` key of the
// channel's `asr.advanced_config` JSON next to `bailianProtocol`.
// Mirrors crates/openless-core/src/asr/qwen_audio.rs: the parser is tolerant field by field
// (bad values fall back to defaults) and the writer preserves every other key.

export type QwenAudioVariant = 'message' | 'streaming' | 'flash' | 'filetrans';

const FLASH_MODEL = 'qwen-audio-3.1-asr-flash';
const DATE_SUFFIX = /^-\d{4}-\d{2}-\d{2}$/;

/** Recognizes the 3.1 family, including dated snapshots (`...-message-2026-10-01`). */
export function qwenAudioVariant(model: string | null | undefined): QwenAudioVariant | null {
  const trimmed = (model ?? '').trim();
  if (!trimmed.startsWith(FLASH_MODEL)) return null;
  const rest = trimmed.slice(FLASH_MODEL.length);
  for (const variant of ['message', 'streaming', 'filetrans'] as const) {
    const suffix = `-${variant}`;
    if (rest.startsWith(suffix)) {
      const tail = rest.slice(suffix.length);
      if (tail === '' || DATE_SUFFIX.test(tail)) return variant;
    }
  }
  return rest === '' || DATE_SUFFIX.test(rest) ? 'flash' : null;
}

export interface QwenAudioCapabilities {
  nativePolish: boolean;
  intermediateResults: boolean;
  /** vad_model, max_sentence_silence, speech_noise_threshold */
  vadTuning: boolean;
}

export function qwenAudioCapabilities(variant: QwenAudioVariant): QwenAudioCapabilities {
  const realtime = variant === 'message' || variant === 'streaming';
  return {
    nativePolish: variant === 'message',
    intermediateResults: variant === 'message',
    vadTuning: realtime,
  };
}

/** `language_hints` codes the 3.1 family accepts. */
export const QWEN_AUDIO_LANGUAGES = [
  'zh',
  'en',
  'ja',
  'ko',
  'vi',
  'th',
  'id',
  'ms',
  'tl',
  'hi',
  'ar',
  'fr',
  'de',
  'es',
  'pt',
  'ru',
  'it',
  'nl',
  'sv',
  'da',
  'fi',
  'no',
  'el',
  'pl',
  'cs',
  'hu',
  'ro',
  'bg',
  'hr',
  'sk',
] as const;
export const MAX_LANGUAGE_HINTS = 4;
export const MIN_SENTENCE_SILENCE_MS = 200;
export const MAX_SENTENCE_SILENCE_MS = 6000;

export type LanguageMode = 'auto' | 'workingLanguages' | 'manual';
export type VadModel = 'near_meeting_16k' | 'far_field_meeting_16k';

export interface QwenAsrOptions {
  disfluencyRemoval: boolean;
  intermediateResults: boolean;
  keepDialect: boolean;
  languageMode: LanguageMode;
  languageHints: string[];
  vadModel: VadModel | null;
  maxSentenceSilenceMs: number | null;
  speechNoiseThreshold: number | null;
  dictionaryHotwords: boolean;
  hotwordWeight: number;
  contextEnhancement: boolean;
}

export const defaultQwenAsrOptions: Readonly<QwenAsrOptions> = Object.freeze({
  disfluencyRemoval: false,
  intermediateResults: true,
  keepDialect: false,
  languageMode: 'auto',
  languageHints: [],
  vadModel: null,
  maxSentenceSilenceMs: null,
  speechNoiseThreshold: null,
  dictionaryHotwords: true,
  hotwordWeight: 4,
  contextEnhancement: false,
});

export function isValidSentenceSilence(value: number): boolean {
  return (
    Number.isInteger(value) && value >= MIN_SENTENCE_SILENCE_MS && value <= MAX_SENTENCE_SILENCE_MS
  );
}

export function isValidNoiseThreshold(value: number): boolean {
  return Number.isFinite(value) && value >= -1 && value <= 1;
}

export function sanitizeLanguageHints(codes: unknown): string[] {
  if (!Array.isArray(codes)) return [];
  const out: string[] = [];
  for (const code of codes) {
    if (typeof code !== 'string') continue;
    const normalized = code.trim().toLowerCase();
    if (
      (QWEN_AUDIO_LANGUAGES as readonly string[]).includes(normalized) &&
      !out.includes(normalized)
    ) {
      out.push(normalized);
    }
    if (out.length === MAX_LANGUAGE_HINTS) break;
  }
  return out;
}

function parseRoot(raw: string | null | undefined): Record<string, unknown> {
  if (!raw?.trim()) return {};
  const value: unknown = JSON.parse(raw);
  if (!value || typeof value !== 'object' || Array.isArray(value))
    throw new Error('Invalid ASR configuration');
  return value as Record<string, unknown>;
}

/** Reads options; malformed JSON or a non-object `qwenAsr` yields defaults. */
export function readQwenAsrOptions(raw: string | null | undefined): QwenAsrOptions {
  let root: Record<string, unknown>;
  try {
    root = parseRoot(raw);
  } catch {
    return { ...defaultQwenAsrOptions, languageHints: [] };
  }
  const value = root.qwenAsr;
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return { ...defaultQwenAsrOptions, languageHints: [] };
  }
  const record = value as Record<string, unknown>;
  const flag = (key: keyof QwenAsrOptions) =>
    typeof record[key] === 'boolean'
      ? (record[key] as boolean)
      : (defaultQwenAsrOptions[key] as boolean);
  const silence = record.maxSentenceSilenceMs;
  const noise = record.speechNoiseThreshold;
  const weight = record.hotwordWeight;
  return {
    disfluencyRemoval: flag('disfluencyRemoval'),
    intermediateResults: flag('intermediateResults'),
    keepDialect: flag('keepDialect'),
    languageMode:
      record.languageMode === 'workingLanguages' || record.languageMode === 'manual'
        ? record.languageMode
        : 'auto',
    languageHints: sanitizeLanguageHints(record.languageHints),
    vadModel:
      record.vadModel === 'near_meeting_16k' || record.vadModel === 'far_field_meeting_16k'
        ? record.vadModel
        : null,
    maxSentenceSilenceMs:
      typeof silence === 'number' && isValidSentenceSilence(silence) ? silence : null,
    speechNoiseThreshold: typeof noise === 'number' && isValidNoiseThreshold(noise) ? noise : null,
    dictionaryHotwords: flag('dictionaryHotwords'),
    hotwordWeight:
      typeof weight === 'number' && Number.isInteger(weight) && weight >= 1 && weight <= 5
        ? weight
        : defaultQwenAsrOptions.hotwordWeight,
    contextEnhancement: flag('contextEnhancement'),
  };
}

/**
 * Writes options into the raw config, keeping every other key (bailianProtocol, ...).
 * Throws on malformed JSON instead of silently discarding what is stored.
 */
export function writeQwenAsrOptions(
  raw: string | null | undefined,
  options: QwenAsrOptions,
): string {
  const root = parseRoot(raw);
  root.qwenAsr = {
    disfluencyRemoval: options.disfluencyRemoval,
    intermediateResults: options.intermediateResults,
    keepDialect: options.keepDialect,
    languageMode: options.languageMode,
    languageHints: sanitizeLanguageHints(options.languageHints),
    vadModel: options.vadModel,
    maxSentenceSilenceMs:
      options.maxSentenceSilenceMs !== null && isValidSentenceSilence(options.maxSentenceSilenceMs)
        ? options.maxSentenceSilenceMs
        : null,
    speechNoiseThreshold:
      options.speechNoiseThreshold !== null && isValidNoiseThreshold(options.speechNoiseThreshold)
        ? options.speechNoiseThreshold
        : null,
    dictionaryHotwords: options.dictionaryHotwords,
    hotwordWeight: options.hotwordWeight,
    contextEnhancement: options.contextEnhancement,
  };
  return JSON.stringify(root);
}
