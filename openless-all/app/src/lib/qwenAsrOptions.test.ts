import {
  defaultQwenAsrOptions,
  qwenAudioCapabilities,
  qwenAudioVariant,
  readQwenAsrOptions,
  writeQwenAsrOptions,
} from './qwenAsrOptions';
import { readBailianProtocol, writeBailianProtocol } from './bailianProtocol';

const assert = {
  equal(actual: unknown, expected: unknown, message = '') {
    if (actual !== expected)
      throw new Error(`${message} Expected ${String(expected)}, received ${String(actual)}`);
  },
  deepEqual(actual: unknown, expected: unknown, message = '') {
    const a = JSON.stringify(actual);
    const e = JSON.stringify(expected);
    if (a !== e) throw new Error(`${message} Expected ${e}, received ${a}`);
  },
  throws(action: () => unknown) {
    try {
      action();
    } catch {
      return;
    }
    throw new Error('Expected an error');
  },
};

// Variant detection mirrors the Rust core.
for (const [model, expected] of [
  ['qwen-audio-3.1-asr-flash-message', 'message'],
  [' qwen-audio-3.1-asr-flash-message-2026-10-01 ', 'message'],
  ['qwen-audio-3.1-asr-flash-streaming', 'streaming'],
  ['qwen-audio-3.1-asr-flash', 'flash'],
  ['qwen-audio-3.1-asr-flash-2026-10-01', 'flash'],
  ['qwen-audio-3.1-asr-flash-filetrans', 'filetrans'],
  ['qwen-audio-3.1-asr-flash-messages', null],
  ['qwen-audio-3.0-asr-flash', null],
  ['fun-asr-realtime', null],
  ['', null],
] as const) {
  assert.equal(qwenAudioVariant(model), expected, model);
}
assert.equal(qwenAudioCapabilities('message').nativePolish, true);
assert.equal(qwenAudioCapabilities('streaming').nativePolish, false);
assert.equal(qwenAudioCapabilities('streaming').vadTuning, true);
assert.equal(qwenAudioCapabilities('flash').vadTuning, false);

// Defaults: native polish off, streaming text on, dictionary hotwords on, context off.
assert.deepEqual(readQwenAsrOptions(null), defaultQwenAsrOptions);
assert.deepEqual(readQwenAsrOptions('broken'), defaultQwenAsrOptions);
assert.deepEqual(readQwenAsrOptions('{"qwenAsr":[]}'), defaultQwenAsrOptions);
assert.equal(defaultQwenAsrOptions.disfluencyRemoval, false);
assert.equal(defaultQwenAsrOptions.intermediateResults, true);
assert.equal(defaultQwenAsrOptions.contextEnhancement, false);

// Out-of-range values fall back individually.
const parsed = readQwenAsrOptions(
  JSON.stringify({
    qwenAsr: {
      disfluencyRemoval: true,
      languageMode: 'manual',
      languageHints: ['ZH', 'xx', 'en', 'en', 'ja', 'ko', 'th'],
      vadModel: 'bogus',
      maxSentenceSilenceMs: 100,
      speechNoiseThreshold: 0.4,
      hotwordWeight: 50,
    },
  }),
);
assert.equal(parsed.disfluencyRemoval, true);
assert.deepEqual(parsed.languageHints, ['zh', 'en', 'ja', 'ko']);
assert.equal(parsed.vadModel, null);
assert.equal(parsed.maxSentenceSilenceMs, null);
assert.equal(parsed.speechNoiseThreshold, 0.4);
assert.equal(parsed.hotwordWeight, 4);

// Writing keeps the protocol and other keys; writing the protocol keeps the options.
const options = { ...defaultQwenAsrOptions, disfluencyRemoval: true, contextEnhancement: true };
const raw = writeQwenAsrOptions(
  '{"bailianProtocol":"dashscope-realtime","enableItn":false}',
  options,
);
assert.equal(readBailianProtocol(raw), 'dashscope-realtime');
assert.equal(JSON.parse(raw).enableItn, false);
assert.deepEqual(readQwenAsrOptions(raw), options);
const reset = writeBailianProtocol(raw, 'auto');
assert.deepEqual(readQwenAsrOptions(reset), options);
assert.equal(readQwenAsrOptions(writeQwenAsrOptions(null, options)).disfluencyRemoval, true);
assert.throws(() => writeQwenAsrOptions('broken JSON', options));

console.log('qwenAsrOptions tests passed');
