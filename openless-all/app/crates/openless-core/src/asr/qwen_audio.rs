//! Qwen-Audio 3.1 ASR family (Alibaba Bailian / DashScope): per-channel
//! options and request-shape helpers shared by the realtime and batch clients.
//!
//! The family has four model ids that use three existing transports:
//!
//! | model                                  | transport                         |
//! |----------------------------------------|-----------------------------------|
//! | `qwen-audio-3.1-asr-flash-message`     | DashScope duplex WS (`bailian.rs`) |
//! | `qwen-audio-3.1-asr-flash-streaming`   | DashScope duplex WS (`bailian.rs`) |
//! | `qwen-audio-3.1-asr-flash`             | sync multimodal-generation HTTP    |
//! | `qwen-audio-3.1-asr-flash-filetrans`   | async file transcription HTTP      |
//!
//! Options live in the channel's `asr.advanced_config` JSON under the
//! `qwenAsr` key, next to `bailianProtocol`, so they ride the existing
//! credential lifecycle (cloud sync, backup, migration). Parsing is
//! conservative: every field that is missing, mistyped, or out of range falls
//! back to its default on its own, so a bad value never reaches the service
//! (an unknown `vad_model`, for example, makes the service fail the task with
//! an opaque `InternalError`).

use serde_json::{json, Map, Value};

pub const MESSAGE_MODEL: &str = "qwen-audio-3.1-asr-flash-message";
pub const STREAMING_MODEL: &str = "qwen-audio-3.1-asr-flash-streaming";
pub const FLASH_MODEL: &str = "qwen-audio-3.1-asr-flash";
pub const FILETRANS_MODEL: &str = "qwen-audio-3.1-asr-flash-filetrans";

/// Key inside `asr.advanced_config`.
pub const OPTIONS_KEY: &str = "qwenAsr";

/// `language_hints` codes the 3.1 family accepts (client-events reference).
pub const SUPPORTED_LANGUAGES: &[&str] = &[
    "zh", "en", "ja", "ko", "vi", "th", "id", "ms", "tl", "hi", "ar", "fr", "de", "es", "pt", "ru",
    "it", "nl", "sv", "da", "fi", "no", "el", "pl", "cs", "hu", "ro", "bg", "hr", "sk",
];
pub const MAX_LANGUAGE_HINTS: usize = 4;
pub const MIN_SENTENCE_SILENCE_MS: u32 = 200;
pub const MAX_SENTENCE_SILENCE_MS: u32 = 6_000;
pub const DEFAULT_HOTWORD_WEIGHT: u8 = 4;
/// Instant and precompiled hotwords are merged server side; past 2000 terms
/// the service samples randomly, so never send more than that.
pub const MAX_VOCABULARY_TERMS: usize = 2_000;
/// Context limits: at most 5 `input_text` messages, 400 characters per turn.
pub const MAX_CONTEXT_TURNS: usize = 5;
pub const MAX_CONTEXT_TURN_CHARS: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QwenAudioVariant {
    /// Realtime, tuned for input methods / voice messages; native polishing.
    Message,
    /// Realtime, general purpose.
    Streaming,
    /// Short recorded audio over sync HTTP.
    Flash,
    /// Long recorded audio over async file transcription.
    Filetrans,
}

impl QwenAudioVariant {
    pub fn is_realtime(self) -> bool {
        matches!(self, Self::Message | Self::Streaming)
    }

    pub fn capabilities(self) -> QwenAudioCapabilities {
        let realtime = self.is_realtime();
        QwenAudioCapabilities {
            native_polish: self == Self::Message,
            intermediate_results: self == Self::Message,
            vad_tuning: realtime,
            language_hints: true,
            keep_dialect: true,
            vocabulary: true,
            context: true,
        }
    }
}

/// Which knobs a variant documents. Only these are ever written to a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QwenAudioCapabilities {
    pub native_polish: bool,
    pub intermediate_results: bool,
    /// `vad_model`, `max_sentence_silence`, `speech_noise_threshold`.
    pub vad_tuning: bool,
    pub language_hints: bool,
    pub keep_dialect: bool,
    pub vocabulary: bool,
    pub context: bool,
}

/// Recognizes the 3.1 family, including possible dated snapshots such as
/// `qwen-audio-3.1-asr-flash-message-2026-10-01`.
pub fn variant(model: &str) -> Option<QwenAudioVariant> {
    let model = model.trim();
    let rest = model.strip_prefix(FLASH_MODEL)?;
    let suffix_is = |name: &str| {
        rest.strip_prefix(name)
            .is_some_and(|tail| tail.is_empty() || is_date_suffix(tail))
    };
    if suffix_is("-message") {
        Some(QwenAudioVariant::Message)
    } else if suffix_is("-streaming") {
        Some(QwenAudioVariant::Streaming)
    } else if suffix_is("-filetrans") {
        Some(QwenAudioVariant::Filetrans)
    } else if rest.is_empty() || is_date_suffix(rest) {
        Some(QwenAudioVariant::Flash)
    } else {
        None
    }
}

fn is_date_suffix(tail: &str) -> bool {
    // "-YYYY-MM-DD"
    let Some(date) = tail.strip_prefix('-') else {
        return false;
    };
    let bytes = date.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LanguageMode {
    /// Send no `language_hints`; the model detects the language.
    #[default]
    Auto,
    /// Map OpenLess working languages to supported codes.
    WorkingLanguages,
    /// Use the channel's own `languageHints` list.
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadModel {
    NearField,
    FarField,
}

impl VadModel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NearField => "near_meeting_16k",
            Self::FarField => "far_field_meeting_16k",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "near_meeting_16k" => Some(Self::NearField),
            "far_field_meeting_16k" => Some(Self::FarField),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QwenAsrOptions {
    /// `disfluency_removal_enabled` (native polishing). Service default: off.
    pub disfluency_removal: bool,
    /// `intermediate_result_enabled`. The service defaults to off for
    /// `-message`; OpenLess defaults it on so the capsule streams text.
    pub intermediate_results: bool,
    pub keep_dialect: bool,
    pub language_mode: LanguageMode,
    pub language_hints: Vec<String>,
    /// `None` keeps the service default (far field).
    pub vad_model: Option<VadModel>,
    /// `None` keeps the service default (800 ms).
    pub max_sentence_silence_ms: Option<u32>,
    /// `None` keeps the service default.
    pub speech_noise_threshold: Option<f64>,
    /// Send OpenLess dictionary entries as instant hotwords (`vocabulary`).
    pub dictionary_hotwords: bool,
    pub hotword_weight: u8,
    /// Send recent dictation transcripts as `input.context`.
    pub context_enhancement: bool,
}

impl Default for QwenAsrOptions {
    fn default() -> Self {
        Self {
            disfluency_removal: false,
            intermediate_results: true,
            keep_dialect: false,
            language_mode: LanguageMode::Auto,
            language_hints: Vec::new(),
            vad_model: None,
            max_sentence_silence_ms: None,
            speech_noise_threshold: None,
            dictionary_hotwords: true,
            hotword_weight: DEFAULT_HOTWORD_WEIGHT,
            context_enhancement: false,
        }
    }
}

/// Reads the `qwenAsr` object out of the raw `asr.advanced_config` JSON.
pub fn parse_options(raw_advanced_config: Option<&str>) -> QwenAsrOptions {
    let defaults = QwenAsrOptions::default();
    let Some(raw) = raw_advanced_config.filter(|raw| !raw.trim().is_empty()) else {
        return defaults;
    };
    let Ok(root) = serde_json::from_str::<Value>(raw) else {
        return defaults;
    };
    let Some(value) = root.get(OPTIONS_KEY).and_then(Value::as_object) else {
        return defaults;
    };
    let flag =
        |key: &str, default: bool| value.get(key).and_then(Value::as_bool).unwrap_or(default);
    QwenAsrOptions {
        disfluency_removal: flag("disfluencyRemoval", defaults.disfluency_removal),
        intermediate_results: flag("intermediateResults", defaults.intermediate_results),
        keep_dialect: flag("keepDialect", defaults.keep_dialect),
        language_mode: match value.get("languageMode").and_then(Value::as_str) {
            Some("workingLanguages") => LanguageMode::WorkingLanguages,
            Some("manual") => LanguageMode::Manual,
            _ => LanguageMode::Auto,
        },
        language_hints: value
            .get("languageHints")
            .and_then(Value::as_array)
            .map(|codes| {
                sanitize_language_codes(codes.iter().filter_map(Value::as_str).map(str::to_string))
            })
            .unwrap_or_default(),
        vad_model: value
            .get("vadModel")
            .and_then(Value::as_str)
            .and_then(VadModel::parse),
        max_sentence_silence_ms: value
            .get("maxSentenceSilenceMs")
            .and_then(Value::as_u64)
            .filter(|ms| {
                (u64::from(MIN_SENTENCE_SILENCE_MS)..=u64::from(MAX_SENTENCE_SILENCE_MS))
                    .contains(ms)
            })
            .map(|ms| ms as u32),
        speech_noise_threshold: value
            .get("speechNoiseThreshold")
            .and_then(Value::as_f64)
            .filter(|threshold| threshold.is_finite() && (-1.0..=1.0).contains(threshold)),
        dictionary_hotwords: flag("dictionaryHotwords", defaults.dictionary_hotwords),
        hotword_weight: value
            .get("hotwordWeight")
            .and_then(Value::as_u64)
            .filter(|weight| (1..=5).contains(weight))
            .map(|weight| weight as u8)
            .unwrap_or(defaults.hotword_weight),
        context_enhancement: flag("contextEnhancement", defaults.context_enhancement),
    }
}

/// Keeps supported codes only, lowercased, de-duplicated, at most four.
fn sanitize_language_codes(codes: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for code in codes {
        let code = code.trim().to_ascii_lowercase();
        if SUPPORTED_LANGUAGES.contains(&code.as_str()) && !out.contains(&code) {
            out.push(code);
        }
        if out.len() == MAX_LANGUAGE_HINTS {
            break;
        }
    }
    out
}

/// Runtime inputs that come from the dictation session rather than the
/// channel configuration.
#[derive(Debug, Clone, Default)]
pub struct QwenAudioSessionInputs<'a> {
    /// OpenLess dictionary entries (already trimmed and de-duplicated).
    pub hotwords: &'a [String],
    /// OpenLess working languages by native name (e.g. "简体中文", "English").
    pub working_languages: &'a [String],
    /// Recent transcripts, oldest first.
    pub context_turns: &'a [String],
    /// The channel's precompiled hotword list id (`asr.vocabulary_id`).
    pub vocabulary_id: Option<&'a str>,
}

/// What a client adds on top of its base request.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QwenAudioRequest {
    /// Merged into the request's `parameters` object.
    pub parameters: Map<String, Value>,
    /// Plain transcripts, oldest first; each client wraps them in its own
    /// envelope (`input.context` for WS / filetrans, leading user
    /// `input_text` messages for sync HTTP).
    pub context_turns: Vec<String>,
}

impl QwenAudioRequest {
    pub fn is_empty(&self) -> bool {
        self.parameters.is_empty() && self.context_turns.is_empty()
    }

    /// `input.context` messages for the duplex WS and filetrans requests.
    pub fn context_messages(&self) -> Vec<Value> {
        self.context_turns
            .iter()
            .map(|text| {
                json!({
                    "role": "user",
                    "content": [{ "type": "input_text", "text": text }],
                })
            })
            .collect()
    }
}

/// Builds the extra request fields for `model` from channel options and
/// session inputs. Returns an empty request for models outside the 3.1
/// family, so callers can apply it unconditionally.
pub fn build_request(
    model: &str,
    options: &QwenAsrOptions,
    inputs: &QwenAudioSessionInputs<'_>,
) -> QwenAudioRequest {
    let Some(variant) = variant(model) else {
        return QwenAudioRequest::default();
    };
    let caps = variant.capabilities();
    let mut parameters = Map::new();

    if caps.native_polish {
        parameters.insert(
            "disfluency_removal_enabled".into(),
            Value::Bool(options.disfluency_removal),
        );
    }
    if caps.intermediate_results {
        parameters.insert(
            "intermediate_result_enabled".into(),
            Value::Bool(options.intermediate_results),
        );
    }
    if caps.keep_dialect && options.keep_dialect {
        parameters.insert("keep_dialect".into(), Value::Bool(true));
    }
    if caps.language_hints {
        let hints = language_hints(options, inputs.working_languages);
        if !hints.is_empty() {
            parameters.insert(
                "language_hints".into(),
                Value::Array(hints.into_iter().map(Value::String).collect()),
            );
        }
    }
    if caps.vad_tuning {
        if let Some(vad) = options.vad_model {
            parameters.insert("vad_model".into(), Value::String(vad.as_str().into()));
        }
        if let Some(ms) = options.max_sentence_silence_ms {
            parameters.insert("max_sentence_silence".into(), Value::from(ms));
        }
        if let Some(threshold) = options.speech_noise_threshold {
            parameters.insert("speech_noise_threshold".into(), Value::from(threshold));
        }
    }
    if caps.vocabulary {
        if options.dictionary_hotwords {
            let vocabulary = vocabulary(inputs.hotwords, options.hotword_weight);
            if !vocabulary.is_empty() {
                parameters.insert("vocabulary".into(), Value::Object(vocabulary));
            }
        }
        if let Some(id) = inputs
            .vocabulary_id
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            parameters.insert("vocabulary_id".into(), Value::String(id.to_string()));
        }
    }

    let context_turns = if caps.context && options.context_enhancement {
        context_turns(inputs.context_turns)
    } else {
        Vec::new()
    };

    QwenAudioRequest {
        parameters,
        context_turns,
    }
}

/// Resolves `language_hints` for the configured mode.
pub fn language_hints(options: &QwenAsrOptions, working_languages: &[String]) -> Vec<String> {
    match options.language_mode {
        LanguageMode::Auto => Vec::new(),
        LanguageMode::Manual => options.language_hints.clone(),
        LanguageMode::WorkingLanguages => sanitize_language_codes(
            working_languages
                .iter()
                .filter_map(|name| crate::language_catalog::asr_language_code(name)),
        ),
    }
}

/// Converts dictionary entries to an instant-hotword map, skipping terms the
/// service rejects: non-ASCII terms longer than 15 characters and ASCII-only
/// terms longer than 7 words.
pub fn vocabulary(hotwords: &[String], weight: u8) -> Map<String, Value> {
    let weight = weight.clamp(1, 5);
    let mut out = Map::new();
    for term in hotwords {
        let term = term.trim();
        if term.is_empty() || out.contains_key(term) {
            continue;
        }
        let fits = if term.is_ascii() {
            term.split_whitespace().count() <= 7
        } else {
            term.chars().count() <= 15
        };
        if !fits {
            continue;
        }
        out.insert(term.to_string(), Value::from(weight));
        if out.len() == MAX_VOCABULARY_TERMS {
            break;
        }
    }
    out
}

/// Keeps the newest `MAX_CONTEXT_TURNS` non-empty turns (input oldest first)
/// and the tail of each turn within `MAX_CONTEXT_TURN_CHARS`: the end of the
/// previous utterance is what the next one most likely continues.
pub fn context_turns(turns: &[String]) -> Vec<String> {
    let kept: Vec<&str> = turns
        .iter()
        .map(|turn| turn.trim())
        .filter(|turn| !turn.is_empty())
        .collect();
    let start = kept.len().saturating_sub(MAX_CONTEXT_TURNS);
    kept[start..]
        .iter()
        .map(|turn| {
            let count = turn.chars().count();
            if count <= MAX_CONTEXT_TURN_CHARS {
                (*turn).to_string()
            } else {
                turn.chars().skip(count - MAX_CONTEXT_TURN_CHARS).collect()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_recognizes_family_and_snapshots_only() {
        assert_eq!(variant(MESSAGE_MODEL), Some(QwenAudioVariant::Message));
        assert_eq!(variant(STREAMING_MODEL), Some(QwenAudioVariant::Streaming));
        assert_eq!(variant(FLASH_MODEL), Some(QwenAudioVariant::Flash));
        assert_eq!(variant(FILETRANS_MODEL), Some(QwenAudioVariant::Filetrans));
        assert_eq!(
            variant(" qwen-audio-3.1-asr-flash-message-2026-10-01 "),
            Some(QwenAudioVariant::Message)
        );
        assert_eq!(
            variant("qwen-audio-3.1-asr-flash-2026-10-01"),
            Some(QwenAudioVariant::Flash)
        );
        for other in [
            "qwen-audio-3.0-asr-flash",
            "qwen-audio-3.0-asr-flash-streaming",
            "qwen-audio-3.1-asr-flash-messages",
            "qwen-audio-3.1-asr-flash-message-x",
            "qwen3-asr-flash",
            "fun-asr-realtime",
            "",
        ] {
            assert_eq!(variant(other), None, "{other}");
        }
    }

    #[test]
    fn missing_or_invalid_config_yields_defaults() {
        let defaults = QwenAsrOptions::default();
        assert!(!defaults.disfluency_removal);
        assert!(defaults.intermediate_results);
        assert!(defaults.dictionary_hotwords);
        assert!(!defaults.context_enhancement);
        for raw in [
            None,
            Some(""),
            Some("not json"),
            Some("[]"),
            Some(r#"{"bailianProtocol":"auto"}"#),
            Some(r#"{"qwenAsr":"yes"}"#),
        ] {
            assert_eq!(parse_options(raw), defaults, "{raw:?}");
        }
    }

    #[test]
    fn parse_reads_every_field_and_rejects_out_of_range_values_individually() {
        let raw = r#"{"bailianProtocol":"dashscope-realtime","qwenAsr":{
            "disfluencyRemoval":true,"intermediateResults":false,"keepDialect":true,
            "languageMode":"manual","languageHints":["ZH","en","xx","en","ja","ko","th"],
            "vadModel":"near_meeting_16k","maxSentenceSilenceMs":1200,
            "speechNoiseThreshold":0.3,"dictionaryHotwords":false,"hotwordWeight":2,
            "contextEnhancement":true}}"#;
        let options = parse_options(Some(raw));
        assert!(options.disfluency_removal);
        assert!(!options.intermediate_results);
        assert!(options.keep_dialect);
        assert_eq!(options.language_mode, LanguageMode::Manual);
        assert_eq!(options.language_hints, ["zh", "en", "ja", "ko"]);
        assert_eq!(options.vad_model, Some(VadModel::NearField));
        assert_eq!(options.max_sentence_silence_ms, Some(1200));
        assert_eq!(options.speech_noise_threshold, Some(0.3));
        assert!(!options.dictionary_hotwords);
        assert_eq!(options.hotword_weight, 2);
        assert!(options.context_enhancement);

        let bad = r#"{"qwenAsr":{"disfluencyRemoval":"true","vadModel":"bogus",
            "maxSentenceSilenceMs":100,"speechNoiseThreshold":1.5,"hotwordWeight":50,
            "languageMode":"weird"}}"#;
        let options = parse_options(Some(bad));
        assert!(!options.disfluency_removal);
        assert_eq!(options.vad_model, None);
        assert_eq!(options.max_sentence_silence_ms, None);
        assert_eq!(options.speech_noise_threshold, None);
        assert_eq!(options.hotword_weight, DEFAULT_HOTWORD_WEIGHT);
        assert_eq!(options.language_mode, LanguageMode::Auto);
    }

    #[test]
    fn message_request_carries_polish_and_intermediate_flags_by_default() {
        let request = build_request(
            MESSAGE_MODEL,
            &QwenAsrOptions::default(),
            &QwenAudioSessionInputs::default(),
        );
        assert_eq!(
            Value::Object(request.parameters),
            json!({"disfluency_removal_enabled": false, "intermediate_result_enabled": true})
        );
        assert!(request.context_turns.is_empty());
    }

    #[test]
    fn variants_only_receive_documented_parameters() {
        let options = QwenAsrOptions {
            disfluency_removal: true,
            keep_dialect: true,
            language_mode: LanguageMode::Manual,
            language_hints: vec!["zh".into()],
            vad_model: Some(VadModel::NearField),
            max_sentence_silence_ms: Some(1000),
            speech_noise_threshold: Some(-0.2),
            context_enhancement: true,
            ..QwenAsrOptions::default()
        };
        let hotwords = vec!["OpenLess".to_string()];
        let turns = vec!["上一句".to_string()];
        let inputs = QwenAudioSessionInputs {
            hotwords: &hotwords,
            working_languages: &[],
            context_turns: &turns,
            vocabulary_id: Some(" vocab-1 "),
        };
        let message = build_request(MESSAGE_MODEL, &options, &inputs);
        assert_eq!(
            Value::Object(message.parameters.clone()),
            json!({
                "disfluency_removal_enabled": true,
                "intermediate_result_enabled": true,
                "keep_dialect": true,
                "language_hints": ["zh"],
                "vad_model": "near_meeting_16k",
                "max_sentence_silence": 1000,
                "speech_noise_threshold": -0.2,
                "vocabulary": {"OpenLess": 4},
                "vocabulary_id": "vocab-1",
            })
        );
        assert_eq!(message.context_turns, ["上一句"]);

        let streaming = build_request(STREAMING_MODEL, &options, &inputs);
        assert!(!streaming
            .parameters
            .contains_key("disfluency_removal_enabled"));
        assert!(!streaming
            .parameters
            .contains_key("intermediate_result_enabled"));
        assert!(streaming.parameters.contains_key("vad_model"));

        for model in [FLASH_MODEL, FILETRANS_MODEL] {
            let batch = build_request(model, &options, &inputs);
            for key in [
                "disfluency_removal_enabled",
                "intermediate_result_enabled",
                "vad_model",
                "max_sentence_silence",
                "speech_noise_threshold",
            ] {
                assert!(!batch.parameters.contains_key(key), "{model} {key}");
            }
            for key in [
                "keep_dialect",
                "language_hints",
                "vocabulary",
                "vocabulary_id",
            ] {
                assert!(batch.parameters.contains_key(key), "{model} {key}");
            }
            assert_eq!(batch.context_turns, ["上一句"]);
        }

        assert!(build_request("fun-asr-realtime", &options, &inputs).is_empty());
    }

    #[test]
    fn working_language_mode_maps_native_names() {
        let options = QwenAsrOptions {
            language_mode: LanguageMode::WorkingLanguages,
            ..QwenAsrOptions::default()
        };
        let languages = vec![
            "简体中文".to_string(),
            "English".to_string(),
            "日本語".to_string(),
        ];
        assert_eq!(language_hints(&options, &languages), ["zh", "en", "ja"]);
        assert!(language_hints(&QwenAsrOptions::default(), &languages).is_empty());
    }

    #[test]
    fn vocabulary_skips_terms_the_service_rejects() {
        let terms: Vec<String> = [
            "OpenLess",
            " 阿里云百炼 ",
            "",
            "OpenLess",
            "一二三四五六七八九十一二三四五六",
            "one two three four five six seven",
            "one two three four five six seven eight",
        ]
        .iter()
        .map(|term| term.to_string())
        .collect();
        let vocabulary = vocabulary(&terms, 9);
        assert_eq!(
            Value::Object(vocabulary),
            json!({"OpenLess": 5, "阿里云百炼": 5, "one two three four five six seven": 5})
        );
    }

    #[test]
    fn context_turns_keep_newest_five_and_tail_of_long_turns() {
        let mut turns: Vec<String> = (1..=7).map(|n| format!("第{n}句")).collect();
        turns.insert(3, "   ".into());
        assert_eq!(
            context_turns(&turns),
            ["第3句", "第4句", "第5句", "第6句", "第7句"]
        );
        let long = format!("{}尾巴", "长".repeat(500));
        let kept = &context_turns(&[long])[0];
        assert_eq!(kept.chars().count(), MAX_CONTEXT_TURN_CHARS);
        assert!(kept.ends_with("尾巴"));
    }

    #[test]
    fn context_messages_use_user_input_text() {
        let request = QwenAudioRequest {
            parameters: Map::new(),
            context_turns: vec!["你好".into()],
        };
        assert_eq!(
            request.context_messages(),
            [json!({"role": "user", "content": [{"type": "input_text", "text": "你好"}]})]
        );
    }
}
