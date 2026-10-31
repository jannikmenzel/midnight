import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { deflateSync } from "node:zlib";

const out = join(dirname(fileURLToPath(import.meta.url)), "..", "public");
const WIDTH = 960;
const HEIGHT = 400;
const GRAIN = 512;

const THEMES = {
  dark: {
    cold: [17, 30, 66],
    mid: [42, 62, 118],
    warm: [186, 98, 58],
    rose: [96, 52, 84],
    strength: 0.9,
    warmth: 0.62,
  },
  light: {
    cold: [176, 206, 236],
    mid: [204, 222, 242],
    warm: [255, 196, 150],
    rose: [247, 196, 196],
    strength: 0.82,
    warmth: 0.9,
  },
};

const fract = (x) => x - Math.floor(x);
const hash = (x, y) => fract(Math.sin(x * 127.1 + y * 311.7) * 43758.5453);
const smooth = (t) => t * t * (3 - 2 * t);
const clamp = (x) => Math.min(1, Math.max(0, x));
const step = (a, b, x) => smooth(clamp((x - a) / (b - a)));
const mix = (a, b, t) => a + (b - a) * t;

function corner(xi, yi, x, y) {
  const angle = hash(xi, yi) * Math.PI * 2;
  return Math.cos(angle) * (x - xi) + Math.sin(angle) * (y - yi);
}

function noise(x, y) {
  const xi = Math.floor(x);
  const yi = Math.floor(y);
  const u = smooth(x - xi);
  const v = smooth(y - yi);
  const top = mix(corner(xi, yi, x, y), corner(xi + 1, yi, x, y), u);
  const bottom = mix(corner(xi, yi + 1, x, y), corner(xi + 1, yi + 1, x, y), u);
  return mix(top, bottom, v) * 0.72 + 0.5;
}

function fbm(x, y, octaves = 4) {
  let sum = 0;
  let amp = 0.5;
  for (let i = 0; i < octaves; i++) {
    sum += amp * noise(x, y);
    const nx = x * 1.62 - y * 1.18 + 7.3;
    y = x * 1.18 + y * 1.62 + 2.9;
    x = nx;
    amp *= 0.46;
  }
  return sum;
}

function crc32(buf) {
  let c = ~0;
  for (const byte of buf) {
    c ^= byte;
    for (let k = 0; k < 8; k++) c = c & 1 ? (c >>> 1) ^ 0xedb88320 : c >>> 1;
  }
  return ~c >>> 0;
}

function png(width, height, channels, data) {
  const chunk = (type, body) => {
    const head = Buffer.alloc(4);
    head.writeUInt32BE(body.length);
    const tagged = Buffer.concat([Buffer.from(type), body]);
    const tail = Buffer.alloc(4);
    tail.writeUInt32BE(crc32(tagged));
    return Buffer.concat([head, tagged, tail]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = channels === 4 ? 6 : 4;
  const rows = Buffer.alloc((width * channels + 1) * height);
  for (let y = 0; y < height; y++) {
    data.copy(rows, y * (width * channels + 1) + 1, y * width * channels, (y + 1) * width * channels);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(rows, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

function renderSky(theme) {
  const data = Buffer.alloc(WIDTH * HEIGHT * 4);
  for (let py = 0; py < HEIGHT; py++) {
    for (let px = 0; px < WIDTH; px++) {
      const u = px / (WIDTH - 1);
      const v = py / (HEIGHT - 1);
      const x = u * 2.2;
      const y = v * 2.6;
      const qx = fbm(x + 1.7, y + 9.2, 3);
      const qy = fbm(x + 8.3, y + 2.8, 3);
      const cloud = step(0.3, 0.68, fbm(x + 1.15 * qx, y + 1.15 * qy));
      const billow = step(0.32, 0.66, fbm(x * 0.8 + 0.9 * qy + 4.1, y * 0.8 + 0.9 * qx + 7.7, 3));

      const dx = (u - 0.76) / 0.46;
      const dy = (v - 0.02) / 0.5;
      const glow = Math.exp(-(dx * dx + dy * dy)) * (0.4 + 0.75 * cloud);
      const flush = Math.exp(-(dx * dx * 0.42 + dy * dy * 0.62)) * (0.45 + 0.55 * billow);

      const fade = 1 - step(0.16, 0.96, v + (0.5 - cloud) * 0.34);
      const body = 0.5 + 0.5 * cloud;
      const alpha = clamp(fade * body * theme.strength);

      const base = theme.cold.map((c, i) => mix(c, theme.mid[i], cloud * 0.9));
      const dusk = base.map((c, i) => mix(c, theme.rose[i], clamp(flush * 1.05)));
      const color = dusk.map((c, i) => mix(c, theme.warm[i], clamp(glow * theme.warmth)));

      const at = (py * WIDTH + px) * 4;
      data[at] = Math.round(color[0]);
      data[at + 1] = Math.round(color[1]);
      data[at + 2] = Math.round(color[2]);
      data[at + 3] = Math.round(alpha * 255);
    }
  }
  return png(WIDTH, HEIGHT, 4, data);
}

function seeded(seed) {
  let state = seed;
  return () => {
    state = (state + 0x6d2b79f5) | 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function renderGrain() {
  const random = seeded(2026);
  const field = new Float64Array(GRAIN * GRAIN);
  for (let i = 0; i < field.length; i++) {
    field[i] = Math.sqrt(-2 * Math.log(Math.max(1e-6, random()))) * Math.cos(2 * Math.PI * random());
  }
  const at = (x, y) => field[((y + GRAIN) % GRAIN) * GRAIN + ((x + GRAIN) % GRAIN)];
  const data = Buffer.alloc(GRAIN * GRAIN * 2);
  for (let y = 0; y < GRAIN; y++) {
    for (let x = 0; x < GRAIN; x++) {
      const n = (at(x, y) * 4 + (at(x - 1, y) + at(x + 1, y) + at(x, y - 1) + at(x, y + 1)) * 2 + at(x - 1, y - 1) + at(x + 1, y - 1) + at(x - 1, y + 1) + at(x + 1, y + 1)) / 7.5;
      const i = (y * GRAIN + x) * 2;
      data[i] = n > 0 ? 255 : 0;
      data[i + 1] = Math.round((Math.min(1.7, Math.abs(n)) * 88) / 16) * 16;
    }
  }
  return png(GRAIN, GRAIN, 2, data);
}

const tmp = mkdtempSync(join(tmpdir(), "midnight-sky-"));
try {
  for (const [name, theme] of Object.entries(THEMES)) {
    const source = join(tmp, `${name}.png`);
    writeFileSync(source, renderSky(theme));
    execFileSync("cwebp", ["-quiet", "-q", "96", "-alpha_q", "100", "-m", "6", source, "-o", join(out, `sky-${name}.webp`)]);
  }
  writeFileSync(join(out, "grain.png"), renderGrain());
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
