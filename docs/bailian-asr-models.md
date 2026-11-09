# 阿里百炼（DashScope）ASR 模型

状态：canonical（2026-09-07 以源码为准重写）；更新：2026-11-09（Qwen-Audio 3.1 系列与参数面板）。

## 1. 代码中的定义

- Provider：`bailian`，`authRequirement = api_key`；默认端点 `wss://dashscope.aliyuncs.com/api-ws/v1/inference/`；默认模型 `fun-asr-realtime`。
- 协议按模型名选择（`provider_rules::resolve_effective_asr_provider`），也可以在渠道里手动指定“接口类型”（存在 `asr.advanced_config` 的 `bailianProtocol`）：
  - 实时（DashScope 双工 WebSocket，`asr/bailian.rs`）：`fun-asr-realtime*`、`paraformer-realtime*`、`sensevoice-realtime*`、`qwen-audio-*-streaming`、`qwen-audio-*-message`；
  - 实时（Qwen Realtime，`asr/qwen_realtime.rs`）：`qwen3-asr-flash-realtime*`；
  - 同步 HTTP（`asr/dashscope_multimodal.rs`）：`fun-asr-flash*`、`qwen3-asr-flash`、`qwen-audio-3.0-asr-flash`、`qwen-audio-3.1-asr-flash`；
  - 异步文件转写（同一文件的异步分支）：`fun-asr`、`fun-asr-*`、`paraformer*`、`qwen-audio-*-filetrans`。
- 验证探针：实时模型发 500 ms 静音（`asr_silence`）；同步 / 异步模型用官方示例音频（`asr_non_silent`）。
- 目录来源：`provider_rules::provider_descriptors` → 生成文件 `src/lib/ipc/provider-descriptors.generated.json`（重新生成：`cargo run --locked -p openless-core --example export_provider_descriptors`）。

## 2. 当前静态模型目录（18 个）

| 模型 | 说明 |
| --- | --- |
| `fun-asr-realtime`（默认） | Fun-ASR 实时转写 |
| `qwen-audio-3.1-asr-flash-message` | Qwen-Audio 3.1，面向输入法 / 语音消息的实时模型，支持原生润色 |
| `qwen-audio-3.1-asr-flash-streaming` | Qwen-Audio 3.1 通用实时模型 |
| `qwen-audio-3.1-asr-flash` | Qwen-Audio 3.1 同步录音识别（单条 ≤ 5 分钟） |
| `qwen-audio-3.1-asr-flash-filetrans` | Qwen-Audio 3.1 异步文件转写 |
| `fun-asr-flash-8k-realtime` | 8k 采样实时版 |
| `fun-asr-flash-2026-06-15`、`fun-asr`、`fun-asr-2025-11-07`、`fun-asr-2025-08-25` | Fun-ASR 各版本 |
| `fun-asr-mtl`、`fun-asr-mtl-2025-08-25` | 多语种（MTL） |
| `qwen3-asr-flash-realtime`、`qwen3-asr-flash-realtime-2026-02-10`、`qwen3-asr-flash-realtime-2025-10-27`、`qwen3-asr-flash` | Qwen3 ASR 实时/闪速 |
| `qwen-audio-3.0-asr-flash` | Qwen Audio 3.0 ASR |
| `paraformer-v2` | Paraformer v2 |

模型 ID 直选与目录以生成文件为准；新增模型先改 Core `provider_rules.rs` 再重新生成目录，不要手工编辑生成文件。

## 3. Qwen-Audio 3.1 参数（`asr/qwen_audio.rs`）

渠道模型是 3.1 系列时，设置页显示“千问 ASR 参数”面板。参数存在 `asr.advanced_config` 的 `qwenAsr` 对象里（与 `bailianProtocol` 共存，随凭据同步 / 备份 / 迁移），解析逐字段保守回落到默认值，越界值不会发给服务端。

| 面板项 | `qwenAsr` 字段 | 请求参数 | 默认 | 适用模型 |
| --- | --- | --- | --- | --- |
| 原生润色 | `disfluencyRemoval` | `disfluency_removal_enabled` | 关 | -message |
| 边说边出字 | `intermediateResults` | `intermediate_result_enabled` | 开（服务端默认关） | -message |
| 识别语言 | `languageMode` + `languageHints` | `language_hints`（≤ 4） | 自动检测 | 全部 |
| 方言处理 | `keepDialect` | `keep_dialect` | 转普通话 | 全部 |
| 使用词典热词 / 权重 | `dictionaryHotwords` / `hotwordWeight` | `vocabulary`（`{词: 1–5}`） | 开 / 4 | 全部 |
| 热词表 ID | `asr.vocabulary_id` | `vocabulary_id` | 空 | 全部 |
| 上下文增强 | `contextEnhancement` | WS / filetrans：`input.context`；同步：音频前的 user `input_text` 消息 | 关 | 全部 |
| 拾音场景 | `vadModel` | `vad_model` | 服务端默认（远场） | 实时 |
| 断句静音时长 | `maxSentenceSilenceMs` | `max_sentence_silence`（200–6000） | 服务端默认 800 | 实时 |
| 噪声阈值 | `speechNoiseThreshold` | `speech_noise_threshold`（-1–1） | 服务端默认 | 实时 |

- 原生润色只作用于每句的最终结果（`sentence_end=true`）：胶囊先显示原话，句末替换成润色后的文字。
- 词典热词按官方限制过滤：含非 ASCII 字符的词条 ≤ 15 字，纯 ASCII 词条 ≤ 7 个单词，最多 2000 条。
- 上下文增强取“对话上下文窗口（分钟）”内最近 5 条语音 / 速记听写的转写（每条取末尾 ≤ 400 字），窗口为 0 时不发送。
- 非法 `vad_model` 会让服务端以不透明的 `InternalError` 结束任务，所以客户端只发送枚举内的值。
- 旧域名 `dashscope.aliyuncs.com` 可直接调用 3.1 系列（2026-11-09 实测）；也可以填业务空间专属域名 `https://{WorkspaceId}.cn-beijing.maas.aliyuncs.com`，后端会按模型换成对应协议和路径。

真实接口冒烟测试：

```sh
DASHSCOPE_API_KEY=sk-... cargo run -p openless-core --example qwen_audio_smoke -- \
  qwen-audio-3.1-asr-flash-message speech.pcm '{"disfluencyRemoval":true}' --hotword OpenLess
```

## 4. 在应用内配置

设置 → AI 服务 → 语音识别 → 添加渠道，选择百炼；填入 API Key（DashScope），选择模型；模型为 Qwen-Audio 3.1 时在下方调整参数；保存后执行“验证”。
