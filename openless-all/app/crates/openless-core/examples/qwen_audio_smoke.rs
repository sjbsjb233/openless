//! Live Qwen-Audio 3.1 (Alibaba Bailian) smoke test through the production path:
//! channel credentials -> `asr.advanced_config` options -> settings Verify -> the
//! shared cloud transcription engine.
//!
//! Run from openless-all/app:
//! DASHSCOPE_API_KEY=sk-... cargo run -p openless-core --example qwen_audio_smoke -- \
//!     <model> speech.pcm ['{"disfluencyRemoval":true}'] \
//!     [--hotword WORD]... [--context TEXT]... [--endpoint URL] [--no-verify]
//! Input must be raw signed 16-bit little-endian, 16 kHz, mono PCM.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use openless_core::credentials::{
    ChannelKind, ChannelMutation, ChannelMutationResult, CredentialKey, CredentialNamespace,
    CredentialStore, InMemoryCredentialStore, SecretValue, ASR_ADVANCED_CONFIG_ACCOUNT,
    ASR_API_KEY_ACCOUNT, ASR_ENDPOINT_ACCOUNT, ASR_MODEL_ACCOUNT,
};
use openless_core::ports::{TextStreamChunk, TextStreamSink, TranscriptionEngine};
use openless_core::{
    BackendError, DictationContext, ProviderApi, ProviderInvocation, ProviderKind, ProviderRequest,
    ProviderService, SessionId, TokioTaskSpawner,
};

#[tokio::main]
async fn main() -> Result<()> {
    let mut positional = Vec::new();
    let mut hotwords = Vec::new();
    let mut context_turns = Vec::new();
    let mut endpoint = None;
    let mut verify = true;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--hotword" => hotwords.push(args.next().context("--hotword needs a value")?),
            "--context" => context_turns.push(args.next().context("--context needs a value")?),
            "--endpoint" => endpoint = Some(args.next().context("--endpoint needs a value")?),
            "--no-verify" => verify = false,
            _ => positional.push(arg),
        }
    }
    let model = positional.first().context("expected <model>")?.clone();
    let path = positional
        .get(1)
        .context("expected a 16 kHz mono s16le PCM file")?;
    let options = positional.get(2).cloned().unwrap_or_else(|| "{}".into());
    let options: serde_json::Value =
        serde_json::from_str(&options).context("options must be a JSON object")?;
    let key = std::env::var("DASHSCOPE_API_KEY").context("DASHSCOPE_API_KEY is required")?;
    let pcm = std::fs::read(path)?;
    anyhow::ensure!(!pcm.is_empty() && pcm.len() % 2 == 0, "invalid PCM length");
    let variant = openless_core::asr::qwen_audio::variant(&model)
        .with_context(|| format!("{model} is not a Qwen-Audio 3.1 model"))?;

    let credentials = Arc::new(InMemoryCredentialStore::default());
    let ChannelMutationResult::Created(channel) = credentials
        .mutate_channel(ChannelMutation::Create {
            kind: ChannelKind::Asr,
            provider_type: "bailian".to_owned(),
            name: "Qwen-Audio smoke test".to_owned(),
        })
        .await?
    else {
        anyhow::bail!("channel creation failed");
    };
    let advanced = serde_json::json!({ "qwenAsr": options }).to_string();
    let mut values = vec![
        (ASR_API_KEY_ACCOUNT, key),
        (ASR_MODEL_ACCOUNT, model.clone()),
        (ASR_ADVANCED_CONFIG_ACCOUNT, advanced.clone()),
    ];
    if let Some(endpoint) = endpoint {
        values.push((ASR_ENDPOINT_ACCOUNT, endpoint));
    }
    for (account, value) in values {
        credentials
            .write(
                CredentialKey::new(CredentialNamespace::Asr, Some(channel.clone()), account)?,
                SecretValue::new(value),
            )
            .await?;
    }
    println!("model={model} variant={variant:?} advanced_config={advanced}");

    if verify {
        let started = Instant::now();
        let service = ProviderService::new(credentials.clone(), Arc::new(TokioTaskSpawner));
        let check = service
            .validate(ProviderRequest {
                kind: ProviderKind::Asr,
                channel_id: Some(channel.clone()),
                thinking_enabled: false,
            })
            .await?;
        anyhow::ensure!(check.ok, "Verify failed");
        println!("Verify succeeded in {} ms.", started.elapsed().as_millis());
    }

    let engine = openless_core::SharedCloudTranscriptionEngine::new(credentials);
    let mut context = DictationContext::default();
    context.asr = ProviderInvocation::for_provider("bailian");
    context.asr.provider_id = channel;
    context.asr.model = Some(model.clone());
    context.polish.hotwords = hotwords;
    context.polish.asr_context_turns = context_turns;
    let preview = Arc::new(Preview::default());
    let started = Instant::now();
    let session = engine
        .start(SessionId::new(), Arc::new(context), preview.clone())
        .await?;
    if variant.is_realtime() {
        for chunk in pcm.chunks(3_200) {
            session.consume_pcm_chunk(chunk);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    } else {
        session.consume_pcm_chunk(&pcm);
    }
    let previews = preview.0.load(Ordering::Acquire);
    let stopped = Instant::now();
    let transcript = session.finish().await?;
    println!(
        "Transcript ({} ms audio, {previews} previews, {} ms after stop, {} ms total): {}",
        transcript.duration_ms,
        stopped.elapsed().as_millis(),
        started.elapsed().as_millis(),
        transcript.text
    );
    anyhow::ensure!(
        !transcript.text.trim().is_empty(),
        "no speech was transcribed"
    );
    Ok(())
}

#[derive(Default)]
struct Preview(AtomicUsize);

impl TextStreamSink for Preview {
    fn publish(&self, chunk: TextStreamChunk) -> Result<(), BackendError> {
        if !chunk.text.is_empty() {
            self.0.fetch_add(1, Ordering::AcqRel);
            println!("  preview: {}", chunk.text);
        }
        Ok(())
    }
}
