import {
  isResponse,
  PROTOCOL_VERSION,
  STATE_FIELDS,
  type HashRequest,
  type LinkCodecPort,
  type ResponseMessage,
  type SimPort,
  type SimResponse,
  type SimCommand,
} from '@rumbo/contracts';

/** State views are host-owned scratch. Subscribers retaining earlier frames must copy. */
export function createWorkerSim(receive: (response: SimResponse) => void): SimPort & LinkCodecPort {
  const worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
  const prev = new Float32Array(STATE_FIELDS.length);
  const curr = new Float32Array(STATE_FIELDS.length);
  let generation = 0;
  let closed = false;
  let ready = false;

  let nextRequest = 0;
  const pending = new Map<number, { resolve: (hash: string) => void; reject: (error: Error) => void }>();
  function rejectPending(message: string) {
    for (const waiter of pending.values()) waiter.reject(new Error(message));
    pending.clear();
  }

  function returnBuffer(msg: Extract<ResponseMessage, { type: 'snapshot' }>) {
    worker.postMessage(
      {
        type: 'returnBuffer',
        protocolVersion: PROTOCOL_VERSION,
        generation: msg.generation,
        slot: msg.slot,
        buffer: msg.buffer,
      },
      [msg.buffer],
    );
  }
  function dispose() {
    if (closed) return;
    closed = true;
    rejectPending('SIM_CLOSED');
    worker.terminate();
  }
  function fail(error: unknown) {
    if (closed) return;
    dispose();
    receive({
      type: 'fatal',
      code: 'SIM_FAILED',
      message: (error instanceof Error ? error.message : String(error)) || 'SIM_FAILED',
    });
  }
  worker.onerror = (event) => fail(event.message || 'WORKER_FAILED');
  worker.onmessageerror = () => fail('INVALID_WORKER_MESSAGE');
  /** Host callbacks run outside the transport guard: a renderer bug must not be reported as a sim crash. */
  function deliver(response: SimResponse) {
    try {
      receive(response);
    } catch (error) {
      console.error('SIM_HOST_CALLBACK_FAILED', error);
    }
  }
  worker.onmessage = (event: MessageEvent<ResponseMessage>) => {
    if (closed) return;
    let out: SimResponse | null = null;
    try {
      const msg = event.data;
      if (!isResponse(msg)) throw new Error('INVALID_RESPONSE');
      // A fatal closes the single worker, even when the host has already queued a replacement load.
      if (msg.type === 'fatal') {
        fail(msg.message);
        return;
      }
      // Content hashes are stateless and keyed by request, so they survive session changes.
      if (msg.type === 'contentHash') {
        pending.get(msg.requestId)?.resolve(msg.hash);
        pending.delete(msg.requestId);
        return;
      }
      if (msg.generation !== generation) {
        if (msg.type === 'snapshot') returnBuffer(msg);
        return;
      }
      if (msg.type === 'ready') ready = true;
      if (msg.type !== 'snapshot') {
        out = msg;
      } else {
        const values = new Float32Array(msg.buffer);
        prev.set(values.subarray(0, prev.length));
        curr.set(values.subarray(prev.length));
        returnBuffer(msg);
        out = {
          type: 'state',
          prev,
          curr,
          at: msg.at,
          stepMs: msg.stepMs,
          simHz: msg.simHz,
          autopilot: msg.autopilot,
          paused: msg.paused,
        };
      }
    } catch (error) {
      fail(error);
      return;
    }
    if (out) deliver(out);
  };
  const finite = (v: number, lo: number, hi: number) => (Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : 0);
  function send(command: SimCommand | HashRequest) {
    if (closed) return;
    if (command.type === 'dispose') {
      dispose();
      return;
    }
    if (command.type === 'newSession' && !ready) {
      deliver({ type: 'commandError', operation: 'newSession', code: 'SIM_NOT_READY' });
      return;
    }
    if (command.type === 'load' && (command.zone !== null || command.race !== null)) {
      // Reserved for later epics: refuse locally so the live session keeps running.
      deliver({ type: 'unsupported', operation: 'load', code: 'UNSUPPORTED_CAPABILITY' });
      return;
    }
    if (command.type === 'load') ready = false;
    if (command.type === 'load' || command.type === 'newSession') generation++;
    if (command.type === 'input') {
      // Sanitise at the host edge: a NaN axis must not become an invalid, fatal protocol frame.
      const f = command.frame;
      command = {
        type: 'input',
        frame: {
          steer: finite(f.steer, -1, 1),
          throttle: finite(f.throttle, 0, 1),
          brake: finite(f.brake, 0, 1),
          handbrake: f.handbrake === true,
        },
      };
    }
    try {
      worker.postMessage({ ...command, protocolVersion: PROTOCOL_VERSION, generation });
    } catch (error) {
      fail(error);
    }
  }
  return {
    send,
    dispose,
    encodeChallenge: async () => ({ ok: false, code: 'UNSUPPORTED_CAPABILITY' }),
    decodeChallenge: async () => ({ ok: false, code: 'UNSUPPORTED_CAPABILITY' }),
    hashContent(bytes) {
      if (closed) return Promise.reject(new Error('SIM_CLOSED'));
      const requestId = ++nextRequest;
      return new Promise((resolve, reject) => {
        pending.set(requestId, { resolve, reject });
        send({ type: 'hashContent', requestId, bytes });
      });
    },
  };
}
