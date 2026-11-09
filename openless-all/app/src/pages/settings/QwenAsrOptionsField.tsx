// Qwen-Audio 3.1 (Alibaba Bailian) recognition options for a unified Bailian ASR channel.
// Shown only when the channel's model is a 3.1 family model; each control appears only for
// the variants whose API documents it (see qwenAudioCapabilities). Values are stored under
// `qwenAsr` in `asr.advanced_config` through the shared write queue.
import { useContext, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { readCredential } from '../../lib/ipc';
import { updateAsrAdvancedConfig } from '../../lib/asrAdvancedConfigQueue';
import {
  MAX_LANGUAGE_HINTS,
  QWEN_AUDIO_LANGUAGES,
  isValidNoiseThreshold,
  isValidSentenceSilence,
  qwenAudioCapabilities,
  readQwenAsrOptions,
  writeQwenAsrOptions,
  defaultQwenAsrOptions,
  type LanguageMode,
  type QwenAsrOptions,
  type QwenAudioVariant,
  type VadModel,
} from '../../lib/qwenAsrOptions';
import { ProviderFormContext } from './ProviderForm';
import { SelectLite } from '../../components/ui/SelectLite';
import { SettingRow, Toggle, inputStyle, btnGhostStyle } from './shared';
import { emitSaved } from '../../lib/savedEvent';

// Distinct from the protocol field's key so the two fields track their own pending saves.
const trackKey = 'asr.advanced_config#qwenAsr';

const noteStyle = { fontSize: 11.5, color: 'var(--ol-ink-4)', lineHeight: 1.6 } as const;

function languageNamer(locale: string): (code: string) => string {
  try {
    const names = new Intl.DisplayNames([locale], { type: 'language' });
    return (code) => names.of(code) ?? code;
  } catch {
    return (code) => code;
  }
}

export function QwenAsrOptionsField({
  channelId,
  variant,
  onUserMutation,
  onBlockedChange,
}: {
  channelId: string;
  variant: QwenAudioVariant;
  onUserMutation?: () => void;
  onBlockedChange: (account: string, blocked: boolean) => void;
}) {
  const { t, i18n } = useTranslation();
  const form = useContext(ProviderFormContext);
  const track = form?.track;
  const register = form?.register;
  const [options, setOptions] = useState<QwenAsrOptions>({ ...defaultQwenAsrOptions });
  const [silenceDraft, setSilenceDraft] = useState('');
  const [noiseDraft, setNoiseDraft] = useState('');
  const [showMore, setShowMore] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const [draftError, setDraftError] = useState('');
  const pending = useRef<Promise<boolean>>(Promise.resolve(true));
  const latest = useRef(options);
  const mounted = useRef(false);
  const canLeave = useRef(false);
  canLeave.current = loaded && !error;
  latest.current = options;

  const caps = qwenAudioCapabilities(variant);
  const nameOf = useMemo(() => languageNamer(i18n.language), [i18n.language]);

  useEffect(() => {
    mounted.current = true;
    let cancelled = false;
    setLoaded(false);
    readCredential('asr.advanced_config', channelId)
      .then((raw) => {
        if (cancelled) return;
        const next = readQwenAsrOptions(raw);
        setOptions(next);
        setSilenceDraft(
          next.maxSentenceSilenceMs === null ? '' : String(next.maxSentenceSilenceMs),
        );
        setNoiseDraft(next.speechNoiseThreshold === null ? '' : String(next.speechNoiseThreshold));
        setLoaded(true);
      })
      .catch((err) => {
        if (!cancelled) setError(String(err));
      });
    return () => {
      cancelled = true;
      mounted.current = false;
    };
  }, [channelId]);

  useEffect(
    () => register?.(trackKey, async () => (await pending.current) && canLeave.current),
    [register],
  );
  useEffect(() => {
    const blocked = !loaded || saving || Boolean(error);
    track?.(trackKey, blocked);
    onBlockedChange(trackKey, blocked);
    return () => {
      track?.(trackKey, false);
      onBlockedChange(trackKey, false);
    };
  }, [loaded, saving, error, track, onBlockedChange]);

  const save = (partial: Partial<QwenAsrOptions>) => {
    if (!loaded) return;
    const next = { ...latest.current, ...partial };
    latest.current = next;
    setOptions(next);
    setSaving(true);
    setError('');
    form?.invalidate(trackKey);
    onUserMutation?.();
    pending.current = (async () => {
      try {
        await updateAsrAdvancedConfig(channelId, (raw) => writeQwenAsrOptions(raw, next));
        if (mounted.current) emitSaved('saved', t('common.saved'));
        return true;
      } catch (err) {
        if (mounted.current) setError(String(err));
        return false;
      } finally {
        if (mounted.current) setSaving(false);
      }
    })();
  };

  const commitSilence = () => {
    const trimmed = silenceDraft.trim();
    if (trimmed === '') {
      setDraftError('');
      if (latest.current.maxSentenceSilenceMs !== null) save({ maxSentenceSilenceMs: null });
      return;
    }
    const value = Number(trimmed);
    if (!isValidSentenceSilence(value)) {
      setDraftError(t('settings.providers.qwenAsr.silenceInvalid'));
      return;
    }
    setDraftError('');
    if (value !== latest.current.maxSentenceSilenceMs) save({ maxSentenceSilenceMs: value });
  };

  const commitNoise = () => {
    const trimmed = noiseDraft.trim();
    if (trimmed === '') {
      setDraftError('');
      if (latest.current.speechNoiseThreshold !== null) save({ speechNoiseThreshold: null });
      return;
    }
    const value = Number(trimmed);
    if (!isValidNoiseThreshold(value)) {
      setDraftError(t('settings.providers.qwenAsr.noiseInvalid'));
      return;
    }
    setDraftError('');
    if (value !== latest.current.speechNoiseThreshold) save({ speechNoiseThreshold: value });
  };

  const disabled = !loaded || Boolean(form?.leaving);
  const k = (key: string) => t(`settings.providers.qwenAsr.${key}`);
  const availableLanguages = QWEN_AUDIO_LANGUAGES.filter(
    (code) => !options.languageHints.includes(code),
  );

  return (
    <div style={{ marginTop: 16 }} data-testid="qwen-asr-options">
      <div style={{ marginBottom: 6 }}>
        <h3
          style={{
            margin: 0,
            fontSize: 14,
            color: 'var(--ol-ink)',
            fontWeight: 'var(--ol-font-weight-semibold)',
          }}
        >
          {k('title')}
        </h3>
        <p style={{ margin: '6px 0 0', color: 'var(--ol-ink-3)', fontSize: 12, lineHeight: 1.65 }}>
          {k('note')}
        </p>
      </div>

      {caps.nativePolish && (
        <SettingRow label={k('nativePolishLabel')} desc={k('nativePolishHint')}>
          <Toggle
            on={options.disfluencyRemoval}
            disabled={disabled}
            onToggle={(next) => save({ disfluencyRemoval: next })}
          />
        </SettingRow>
      )}
      {caps.intermediateResults && (
        <SettingRow label={k('intermediateLabel')} desc={k('intermediateHint')}>
          <Toggle
            on={options.intermediateResults}
            disabled={disabled}
            onToggle={(next) => save({ intermediateResults: next })}
          />
        </SettingRow>
      )}

      <SettingRow label={k('languageLabel')} desc={k('languageHint')}>
        <SelectLite
          ariaLabel={k('languageLabel')}
          value={options.languageMode}
          disabled={disabled}
          onChange={(mode) => save({ languageMode: mode as LanguageMode })}
          options={(['auto', 'workingLanguages', 'manual'] as const).map((mode) => ({
            value: mode,
            label: t(`settings.providers.qwenAsr.languageModes.${mode}`),
          }))}
        />
      </SettingRow>
      {options.languageMode === 'manual' && (
        <div
          style={{
            display: 'flex',
            flexWrap: 'wrap',
            gap: 6,
            alignItems: 'center',
            margin: '4px 0 8px',
          }}
        >
          {options.languageHints.map((code) => (
            <button
              key={code}
              type="button"
              style={btnGhostStyle}
              disabled={disabled}
              aria-label={t('settings.providers.qwenAsr.languageRemove', { name: nameOf(code) })}
              onClick={() =>
                save({ languageHints: options.languageHints.filter((item) => item !== code) })
              }
            >
              {nameOf(code)} ×
            </button>
          ))}
          {options.languageHints.length < MAX_LANGUAGE_HINTS ? (
            <div style={{ minWidth: 180 }}>
              <SelectLite
                ariaLabel={k('languageAdd')}
                value=""
                placeholder={k('languageAdd')}
                searchable
                disabled={disabled}
                onChange={(code) => save({ languageHints: [...options.languageHints, code] })}
                options={availableLanguages.map((code) => ({
                  value: code,
                  label: `${nameOf(code)} (${code})`,
                }))}
              />
            </div>
          ) : (
            <span style={noteStyle}>{k('languageLimit')}</span>
          )}
          {options.languageHints.length === 0 && (
            <span style={noteStyle}>{k('languageEmpty')}</span>
          )}
        </div>
      )}

      <SettingRow label={k('dialectLabel')} desc={k('dialectHint')}>
        <SelectLite
          ariaLabel={k('dialectLabel')}
          value={options.keepDialect ? 'keep' : 'mandarin'}
          disabled={disabled}
          onChange={(value) => save({ keepDialect: value === 'keep' })}
          options={(['mandarin', 'keep'] as const).map((value) => ({
            value,
            label: t(`settings.providers.qwenAsr.dialectOptions.${value}`),
          }))}
        />
      </SettingRow>

      <SettingRow label={k('hotwordsLabel')} desc={k('hotwordsHint')}>
        <Toggle
          on={options.dictionaryHotwords}
          disabled={disabled}
          onToggle={(next) => save({ dictionaryHotwords: next })}
        />
      </SettingRow>
      {options.dictionaryHotwords && (
        <SettingRow label={k('hotwordWeightLabel')} desc={k('hotwordWeightHint')}>
          <SelectLite
            ariaLabel={k('hotwordWeightLabel')}
            value={String(options.hotwordWeight)}
            disabled={disabled}
            onChange={(value) => save({ hotwordWeight: Number(value) })}
            options={[1, 2, 3, 4, 5].map((weight) => ({
              value: String(weight),
              label: String(weight),
            }))}
          />
        </SettingRow>
      )}

      <SettingRow label={k('contextLabel')} desc={k('contextHint')}>
        <Toggle
          on={options.contextEnhancement}
          disabled={disabled}
          onToggle={(next) => save({ contextEnhancement: next })}
        />
      </SettingRow>

      {caps.vadTuning && (
        <>
          <button
            type="button"
            style={{ ...btnGhostStyle, marginTop: 8 }}
            aria-expanded={showMore}
            onClick={() => setShowMore((value) => !value)}
          >
            {showMore ? k('moreHide') : k('moreShow')}
          </button>
          {showMore && (
            <>
              <SettingRow label={k('vadLabel')} desc={k('vadHint')}>
                <SelectLite
                  ariaLabel={k('vadLabel')}
                  value={options.vadModel ?? 'default'}
                  disabled={disabled}
                  onChange={(value) =>
                    save({ vadModel: value === 'default' ? null : (value as VadModel) })
                  }
                  options={[
                    { value: 'default', label: k('vadOptions.default') },
                    { value: 'near_meeting_16k', label: k('vadOptions.near') },
                    { value: 'far_field_meeting_16k', label: k('vadOptions.far') },
                  ]}
                />
              </SettingRow>
              <SettingRow label={k('silenceLabel')} desc={k('silenceHint')}>
                <input
                  type="number"
                  min={200}
                  max={6000}
                  step={100}
                  value={silenceDraft}
                  placeholder="800"
                  disabled={disabled}
                  aria-label={k('silenceLabel')}
                  onChange={(event) => setSilenceDraft(event.target.value)}
                  onBlur={commitSilence}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') (event.target as HTMLInputElement).blur();
                  }}
                  style={inputStyle}
                />
              </SettingRow>
              <SettingRow label={k('noiseLabel')} desc={k('noiseHint')}>
                <input
                  type="number"
                  min={-1}
                  max={1}
                  step={0.1}
                  value={noiseDraft}
                  placeholder={k('noisePlaceholder')}
                  disabled={disabled}
                  aria-label={k('noiseLabel')}
                  onChange={(event) => setNoiseDraft(event.target.value)}
                  onBlur={commitNoise}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') (event.target as HTMLInputElement).blur();
                  }}
                  style={inputStyle}
                />
              </SettingRow>
            </>
          )}
        </>
      )}

      {draftError && (
        <div role="alert" style={{ fontSize: 11, color: 'var(--ol-warn)', lineHeight: 1.4 }}>
          {draftError}
        </div>
      )}
      {error && (
        <div role="alert" style={{ fontSize: 11, color: 'var(--ol-warn)', lineHeight: 1.4 }}>
          {t('common.operationFailed')}: {error}
        </div>
      )}
    </div>
  );
}
