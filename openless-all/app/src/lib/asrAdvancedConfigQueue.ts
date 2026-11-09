// Several settings fields share one stored JSON value (`asr.advanced_config`): the Bailian
// protocol selector and the Qwen-Audio options panel. Each change is a read-merge-write, so
// two fields saving at once could interleave and drop the other's change. Run the
// read-merge-write steps one at a time per channel.
import { readCredential, setCredential } from './ipc';

export const ASR_ADVANCED_CONFIG_ACCOUNT = 'asr.advanced_config';

const queues = new Map<string, Promise<unknown>>();

export function updateAsrAdvancedConfig(
  channelId: string,
  mutate: (raw: string | null) => string,
): Promise<string> {
  const previous = queues.get(channelId) ?? Promise.resolve();
  const next = previous
    .catch(() => undefined)
    .then(async () => {
      const raw = await readCredential(ASR_ADVANCED_CONFIG_ACCOUNT, channelId);
      const updated = mutate(raw);
      await setCredential(ASR_ADVANCED_CONFIG_ACCOUNT, updated, channelId);
      return updated;
    });
  queues.set(channelId, next);
  void next.finally(() => {
    if (queues.get(channelId) === next) queues.delete(channelId);
  });
  return next;
}
