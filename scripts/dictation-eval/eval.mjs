// Offline dictation eval: TTS speech -> Deepgram Nova-3 (same params as Android) -> agent cleanup.
// Usage: node scripts/dictation-eval/eval.mjs [filter] [--voices=a,b] [--no-agent]
// Uses the app's embedded Deepgram key (decoded from Secrets.kt, never printed).
import fs from 'node:fs';
import WebSocket from 'ws';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '../..');
const shared = path.join(repo, 'shared/dictation');
const cacheDir = path.join(here, '.cache');
fs.mkdirSync(cacheDir, { recursive: true });

const args = process.argv.slice(2);
const filter = args.find((a) => !a.startsWith('--')) ?? '';
const voices = (args.find((a) => a.startsWith('--voices='))?.slice(9) ?? 'aura-2-orion-en,aura-2-thalia-en').split(',');
const PACE_MS = args.includes('--realtime') ? 100 : 25;
const useAgent = !args.includes('--no-agent');
const model = args.find((a) => a.startsWith('--model='))?.slice(8) ?? 'gpt-5.4-mini';

function deepgramKey() {
  const src = fs.readFileSync(path.join(repo, 'android/app/src/main/java/com/maxspeech/android/data/Secrets.kt'), 'utf8');
  const arr = (name) => [...src.match(new RegExp(`${name} = intArrayOf\\(([^)]*)\\)`))[1].matchAll(/0x[0-9A-Fa-f]+/g)].map((m) => parseInt(m[0], 16));
  const mask = arr('MASK');
  return Buffer.from(arr('BYTES').map((b, i) => b ^ mask[i % mask.length])).toString('utf8');
}
const KEY = process.env.DEEPGRAM_API_KEY || deepgramKey();

const keyterms = fs.readFileSync(path.join(shared, 'keyterms.txt'), 'utf8')
  .split(/\r?\n/).map((l) => l.trim()).filter((l) => l && !l.startsWith('#'));
const cases = JSON.parse(fs.readFileSync(path.join(here, 'cases.json'), 'utf8'))
  .filter((c) => !filter || c.id.includes(filter));

async function tts(text, voice) {
  const file = path.join(cacheDir, `${voice}-${Buffer.from(text).toString('base64url').slice(0, 80)}.pcm`);
  if (fs.existsSync(file)) return fs.readFileSync(file);
  const res = await fetch(`https://api.deepgram.com/v1/speak?model=${voice}&encoding=linear16&sample_rate=16000&container=none`, {
    method: 'POST',
    headers: { Authorization: `Token ${KEY}`, 'Content-Type': 'application/json' },
    body: JSON.stringify({ text }),
  });
  if (!res.ok) throw new Error(`tts ${res.status} ${await res.text()}`);
  const buf = Buffer.from(await res.arrayBuffer());
  fs.writeFileSync(file, buf);
  return buf;
}

const sttParams = 'model=nova-3&language=en&punctuate=true&smart_format=true&numerals=false&encoding=linear16&sample_rate=16000&channels=1'
  + keyterms.map((t) => `&keyterm=${encodeURIComponent(t)}`).join('');

async function sttBatch(pcm) {
  const res = await fetch(`https://api.deepgram.com/v1/listen?${sttParams}`, {
    method: 'POST',
    headers: { Authorization: `Token ${KEY}`, 'Content-Type': 'application/octet-stream' },
    body: pcm,
  });
  if (!res.ok) throw new Error(`stt ${res.status} ${await res.text()}`);
  return (await res.json()).results.channels[0].alternatives[0].transcript;
}

// Mirrors DeepgramClient's streaming socket: commit is_final segments in order.
function sttStream(pcm) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`wss://api.deepgram.com/v1/listen?${sttParams}&interim_results=true&endpointing=1100`, { headers: { Authorization: `Token ${KEY}` } });
    const finals = [];
    ws.onopen = async () => {
      const chunk = 3200; // 100 ms
      for (let i = 0; i < pcm.length; i += chunk) {
        ws.send(pcm.subarray(i, i + chunk));
        await new Promise((r) => setTimeout(r, PACE_MS));
      }
      ws.send(new Uint8Array(16000)); // short silent tail like the release trail
      ws.send(JSON.stringify({ type: 'Finalize' }));
      ws.send(JSON.stringify({ type: 'CloseStream' }));
    };
    ws.onmessage = (e) => { if (e.data instanceof Buffer && e.data[0] !== 123) return;
      const m = JSON.parse(e.data);
      if (m.type === 'Results' && m.is_final) {
        const t = m.channel.alternatives[0].transcript;
        if (t) finals.push(t);
      }
    };
    ws.onerror = (e) => reject(new Error(`stream error ${e.message ?? ''}`));
    ws.onclose = () => resolve(finals.join(' '));
  });
}

// --level=light|high: read the strength text from the Kotlin source so the eval matches the app.
const level = args.find((a) => a.startsWith('--level='))?.slice(8) ?? 'medium';
function levelLine() {
  const lines = fs.readFileSync(path.join(repo, 'android/app/src/main/java/com/maxspeech/android/pipeline/AgentEnhanceClient.kt'), 'utf8').split(/\r?\n/);
  const i = lines.findIndex((l) => l.trim().startsWith(`"${level}" ->`));
  if (i < 0) return '';
  const strs = [lines[i], lines[i + 1]].join(' ').match(/"((?:[^"\\]|\\.)*)"/g).slice(1);
  return `\nStrength: ${strs.map((s) => s.slice(1, -1).replace(/\\"/g, '"')).join('')}`;
}

function systemPrompt(app) {
  return `${fs.readFileSync(path.join(shared, 'agent_cleanup.txt'), 'utf8')}\n\nStyle: natural and clean, in the speaker's own voice.${levelLine()}\n\nContext (reference only, never copy it into the output):\nApp: ${app}`;
}

// Same Voice Agent + deliver_text function trick as AgentEnhanceClient.kt.
function agentCleanup(text, app = 'Messages') {
  return new Promise((resolve) => {
    const ws = new WebSocket('wss://agent.deepgram.com/v1/agent/converse', { headers: { Authorization: `Token ${KEY}` } });
    const t0 = Date.now();
    let keep;
    const done = (v) => { clearInterval(keep); try { ws.close(); } catch {} resolve({ text: v, ms: Date.now() - t0 }); };
    const timer = setTimeout(() => done(null), 15000);
    ws.onmessage = (e) => { if (e.data instanceof Buffer && e.data[0] !== 123) return;
      if (typeof e.data !== 'string') return;
      const m = JSON.parse(e.data);
      if (m.type === 'Welcome') {
        ws.send(JSON.stringify({
          type: 'Settings',
          audio: { input: { encoding: 'linear16', sample_rate: 16000 }, output: { encoding: 'linear16', sample_rate: 16000, container: 'none' } },
          agent: {
            language: 'en',
            listen: { provider: { type: 'deepgram', model: 'nova-3' } },
            think: {
              provider: { type: 'open_ai', model },
              prompt: systemPrompt(app),
              functions: [{ name: 'deliver_text', description: 'Deliver the final cleaned dictation text to be typed.', parameters: { type: 'object', properties: { text: { type: 'string' } }, required: ['text'] } }],
            },
            speak: { provider: { type: 'deepgram', model: 'aura-2-thalia-en' } },
          },
        }));
      } else if (m.type === 'SettingsApplied') {
        keep = setInterval(() => ws.send(new Uint8Array(3840)), 120);
        ws.send(JSON.stringify({ type: 'InjectUserMessage', content: `<t>${text}</t>` }));
      } else if (m.type === 'FunctionCallRequest') {
        clearTimeout(timer);
        done(JSON.parse(m.functions[0].arguments).text.trim());
      } else if (m.type === 'Error') {
        clearTimeout(timer);
        done(`ERROR ${m.description ?? m.message}`);
      }
    };
    ws.onerror = () => { clearTimeout(timer); done(null); };
  });
}

const check = (c, text) => (text ?? '') && new RegExp(c.expect, 'i').test(text) && !(c.reject && new RegExp(c.reject, 'i').test(text));

// Same whole-phrase fixes Vocab.fixCommonAsr applies before cleanup (shared file).
const phraseFixes = fs.readFileSync(path.join(shared, 'asr_phrase_fixes.txt'), 'utf8').split(/\r?\n/)
  .map((l) => l.trim()).filter((l) => l && !l.startsWith('#') && l.includes('=>'))
  .map((l) => l.split('=>').map((s) => s.trim()));
const esc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const applyFixes = (t) => phraseFixes.reduce((acc, [from, to]) =>
  acc.replace(new RegExp(`(?<![\\p{L}\\p{N}'])${esc(from)}(?![\\p{L}\\p{N}'])`, 'giu'), (m) => (/\p{Lu}/u.test(m[0]) ? to[0].toUpperCase() + to.slice(1) : to)), t);
const AI_MIN_WORDS = 3; // DictationController skips the AI for very short dictations

const rows = [];
for (const c of cases) {
  for (const voice of voices) {
    const pcm = await tts(c.say, voice);
    const [batch, stream] = await Promise.all([sttBatch(pcm), sttStream(pcm)]);
    const input = applyFixes(c.input ?? stream);
    const short = input.split(/\s+/).filter(Boolean).length <= AI_MIN_WORDS;
    const agent = useAgent ? (short ? { text: input, ms: 0 } : await agentCleanup(input, c.app)) : null;
    const row = { id: c.id, voice: voice.split('-')[2], stream, batch, input, agent: agent?.text, ms: agent?.ms, sttOk: check(c, input), finalOk: agent ? check(c, agent.text) : null };
    rows.push(row);
    console.log(`${row.finalOk === false || (!useAgent && !row.sttOk) ? 'FAIL' : 'ok  '} ${c.id} [${row.voice}]\n   stream: ${stream}\n   batch : ${batch}\n   input : ${input}${agent ? `\n   final : ${agent.text} (${short ? 'no AI, short' : `${agent.ms} ms`})` : ''}`);
  }
}
const n = rows.length;
console.log(`\nSTT pass ${rows.filter((r) => r.sttOk).length}/${n}` + (useAgent ? `, final pass ${rows.filter((r) => r.finalOk).length}/${n}` : ''));
fs.writeFileSync(path.join(here, 'last-run.json'), JSON.stringify(rows, null, 2));
