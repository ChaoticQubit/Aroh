import { addPluginListener, invoke, type PluginListener } from '@tauri-apps/api/core';

/** Error codes the `aroh-speech` plugin rejects with, plus `unknown` for anything else. */
export type SpeechErrorCode =
  | 'unsupported'
  | 'permission_denied'
  | 'on_device_unavailable'
  | 'recognizer_unavailable'
  | 'busy'
  | 'not_started'
  | 'audio_failed'
  | 'unknown';

const KNOWN_CODES: readonly SpeechErrorCode[] = [
  'unsupported',
  'permission_denied',
  'on_device_unavailable',
  'recognizer_unavailable',
  'busy',
  'not_started',
  'audio_failed',
];

/** A speech plugin failure with a stable `code` and a message fit to show the owner. */
export class SpeechError extends Error {
  readonly code: SpeechErrorCode;

  constructor(code: SpeechErrorCode, message: string) {
    super(message);
    this.code = code;
  }
}

/** Voice input the log screen needs; `speech` below is the Tauri implementation. */
export interface Speech {
  start(onPartial: (text: string) => void): Promise<void>;
  stop(): Promise<string>;
}

/** Maps a plugin rejection (`{code, message}` object or plain string) to a `SpeechError`. */
function toSpeechError(raw: unknown): SpeechError {
  if (raw instanceof SpeechError) return raw;
  if (typeof raw === 'object' && raw !== null) {
    const { code, message } = raw as { code?: unknown; message?: unknown };
    const known = KNOWN_CODES.find((c) => c === code);
    return new SpeechError(known ?? 'unknown', typeof message === 'string' ? message : 'Voice input failed.');
  }
  return new SpeechError('unknown', typeof raw === 'string' ? raw : 'Voice input failed.');
}

let partialListener: PluginListener | null = null;

/** Drops the partial-result listener, if any. */
async function dropListener(): Promise<void> {
  const listener = partialListener;
  partialListener = null;
  await listener?.unregister().catch(() => undefined);
}

/**
 * The one place the log screen touches `@tauri-apps/api`.
 * `start` begins on-device recognition and streams partial text to `onPartial`;
 * `stop` resolves the final transcript. Both reject with `SpeechError`.
 */
export const speech: Speech = {
  async start(onPartial) {
    await dropListener();
    try {
      await invoke('plugin:aroh-speech|start');
    } catch (raw) {
      throw toSpeechError(raw);
    }
    partialListener = await addPluginListener<{ text: string }>('aroh-speech', 'partial', (p) =>
      onPartial(p.text),
    ).catch(() => null);
  },
  async stop() {
    try {
      const result = await invoke<{ transcript: string }>('plugin:aroh-speech|stop');
      return result.transcript;
    } catch (raw) {
      throw toSpeechError(raw);
    } finally {
      await dropListener();
    }
  },
};
