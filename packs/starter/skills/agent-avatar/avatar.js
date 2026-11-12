// Turns one generated picture into an avatar sheet Razor draws, checks a sheet, and installs one.
// A sheet is a 132 × 33 RGBA PNG of four 33 px cells (idle, blink, talk, sleep) with alpha 0 or
// 255, at most 16 colours and 2 KiB. Plain Node 22.18 or later, no packages.
'use strict';
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
const crypto = require('node:crypto'), http = require('node:http'), zlib = require('node:zlib');
const {spawnSync} = require('node:child_process'), {parseArgs} = require('node:util');

const CELL = 33, SIGNATURE = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
const DAMAGED = 'the PNG is damaged; save it again as 8-bit RGBA or make another';
const SMALLER = 'make it with this script from a picture with fewer colours';
const USAGE = `usage: node avatar.js make <picture.png> character|emblem <sheet.png>
       node avatar.js check <sheet.png>
       node avatar.js install <sheet.png> <name> [--pool] [--resident <name>] [--session <id>]`;
const lum = c => c[0] * .2126 + c[1] * .7152 + c[2] * .0722;
const distance = (a, b) => (a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2 + (a[2] - b[2]) ** 2;
const nearest = (c, palette) => palette.reduce((a, b) => distance(c, a) <= distance(c, b) ? a : b);

// A file's bytes, refused unread past `limit` bytes with `advice`.
function read(file, limit, advice) {
  const stat = fs.statSync(file, {throwIfNoEntry: false});
  if (!stat?.isFile() || stat.size > limit) throw Error(stat?.isFile() ? `${file} is ${stat.size} bytes, over ${limit}; ${advice}` : `there is no file ${file}`);
  return fs.readFileSync(file);
}

// An 8-bit, non-interlaced PNG of any colour type, as {w, h, type, pixels} with RGBA pixels.
function decode(bytes) {
  if (!bytes.subarray(0, 8).equals(SIGNATURE)) throw Error('not a PNG; convert the picture to PNG first');
  let w, h, type, channels, palette, alpha;
  const data = [];
  for (let p = 8; ; p += bytes.readUInt32BE(p) + 12) {
    if (p + 12 > bytes.length) throw Error(DAMAGED);
    const n = bytes.readUInt32BE(p), tag = bytes.toString('latin1', p + 4, p + 8), d = bytes.subarray(p + 8, p + 8 + n);
    if (p + 12 + n > bytes.length || zlib.crc32(bytes.subarray(p + 4, p + 8 + n)) !== bytes.readUInt32BE(p + 8 + n))
      throw Error(DAMAGED);
    if (tag === 'IHDR') {
      [w, h, type] = [d.readUInt32BE(0), d.readUInt32BE(4), d[9]];
      channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[type];
      if (d[8] !== 8 || !channels || d[12] || w * h > 2e7)
        throw Error('only 8-bit, non-interlaced PNGs up to 20 megapixels are read; save it again as 8-bit RGBA');
    } else if (tag === 'PLTE') palette = d;
    else if (tag === 'tRNS') alpha = d;
    else if (tag === 'IDAT') data.push(d);
    else if (tag === 'IEND') break;
  }
  if (!w || !data.length) throw Error('the PNG has no image');
  if (alpha && type !== 3) throw Error('the PNG marks transparency by colour key; save it again as RGBA');
  const stride = w * channels, line = stride + 1;
  let raw;
  try { raw = zlib.inflateSync(Buffer.concat(data), {maxOutputLength: line * h}); } catch {}
  if (raw?.length !== line * h) throw Error(DAMAGED);
  const scan = Buffer.alloc(stride * h), pixels = Buffer.alloc(w * h * 4);
  for (let y = 0; y < h; y++) {
    const filter = raw[y * line];
    if (filter > 4) throw Error(DAMAGED);
    for (let x = 0; x < stride; x++) {
      const i = y * stride + x, a = x >= channels ? scan[i - channels] : 0, u = y ? scan[i - stride] : 0;
      const ul = y && x >= channels ? scan[i - stride - channels] : 0, e = a + u - ul;
      const paeth = Math.abs(e - a) <= Math.abs(e - u) && Math.abs(e - a) <= Math.abs(e - ul) ? a
        : Math.abs(e - u) <= Math.abs(e - ul) ? u : ul;
      scan[i] = raw[y * line + 1 + x] + [0, a, u, (a + u) >> 1, paeth][filter];
    }
  }
  for (let i = 0, s = 0, o = 0; i < w * h; i++, s += channels, o += 4) {
    if (type === 6) scan.copy(pixels, o, s, s + 4);
    else if (type === 2) { scan.copy(pixels, o, s, s + 3); pixels[o + 3] = 255; }
    else if (type === 3) {
      if (!palette || scan[s] * 3 + 3 > palette.length) throw Error(DAMAGED);
      palette.copy(pixels, o, scan[s] * 3, scan[s] * 3 + 3);
      pixels[o + 3] = alpha?.[scan[s]] ?? 255;
    } else { pixels.fill(scan[s], o, o + 3); pixels[o + 3] = type === 4 ? scan[s + 1] : 255; }
  }
  return {w, h, type, pixels};
}

function chunk(tag, data) {
  const out = Buffer.concat([Buffer.alloc(4), Buffer.from(tag, 'latin1'), data, Buffer.alloc(4)]);
  out.writeUInt32BE(data.length);
  out.writeUInt32BE(zlib.crc32(out.subarray(4, -4)), out.length - 4);
  return out;
}

function encode({w, h, pixels}) {
  const header = Buffer.from([0, 0, 0, 0, 0, 0, 0, 0, 8, 6, 0, 0, 0]), raw = Buffer.alloc((w * 4 + 1) * h);
  header.writeUInt32BE(w);
  header.writeUInt32BE(h, 4);
  for (let y = 0; y < h; y++) pixels.copy(raw, y * (w * 4 + 1) + 1, y * w * 4, (y + 1) * w * 4);
  return Buffer.concat([SIGNATURE, chunk('IHDR', header), chunk('IDAT', zlib.deflateSync(raw, {level: 9})),
    chunk('IEND', Buffer.alloc(0))]);
}

// A picture with no transparent pixel stands on a plain ground: its corner colour, flooded in from
// every edge, becomes clear. A ground whose corners differ is not plain and is refused.
function clearGround({w, h, pixels: p}) {
  if (p.some((v, i) => i % 4 === 3 && v < 128)) return;
  const at = i => [p[i * 4], p[i * 4 + 1], p[i * 4 + 2]], ground = at(0);
  if ([w - 1, (h - 1) * w, w * h - 1].some(i => distance(at(i), ground) >= 900))
    throw Error('the picture has no transparent background and its corners differ; ask for a transparent or plain one-colour background');
  const seen = new Uint8Array(w * h), queue = [];
  const visit = i => { if (!seen[i]) { seen[i] = 1; if (distance(at(i), ground) < 900) queue.push(i); } };
  for (let x = 0; x < w; x++) { visit(x); visit((h - 1) * w + x); }
  for (let y = 0; y < h; y++) { visit(y * w); visit(y * w + w - 1); }
  for (const i of queue) {
    p[i * 4 + 3] = 0;
    for (const j of [i % w ? i - 1 : -1, (i + 1) % w ? i + 1 : -1, i - w, i + w]) if (j >= 0 && j < w * h) visit(j);
  }
}

// Median cut over the blocks' colours, weighted by how many blocks carry each; every box answers
// with its most common colour, so no averaged colour is invented.
function paletteOf(counts, size) {
  const boxes = [[...counts].map(([k, n]) => ({c: k.split(',').map(Number), n}))];
  const range = box => [0, 1, 2].map(i => Math.max(...box.map(v => v.c[i])) - Math.min(...box.map(v => v.c[i])));
  const weight = box => box.reduce((s, v) => s + v.n, 0);
  while (boxes.length < size) {
    const split = boxes.filter(b => b.length > 1).sort((a, b) =>
      Math.max(...range(b)) * Math.sqrt(weight(b)) - Math.max(...range(a)) * Math.sqrt(weight(a)))[0];
    if (!split) break;
    const r = range(split), axis = r.indexOf(Math.max(...r)), half = weight(split) / 2;
    split.sort((a, b) => a.c[axis] - b.c[axis]);
    let j = 0;
    for (let n = 0; n < half && j < split.length - 1; ) n += split[j++].n;
    boxes.splice(boxes.indexOf(split), 1, split.slice(0, j), split.slice(j));
  }
  return boxes.map(b => b.sort((x, y) => y.n - x.n)[0].c);
}

// The subject cropped, fitted within 29 × 29 at the middle of a cell, each block taking its most
// common colour, then held to `size` colours. Returns the cell as 33 × 33 colours or nulls.
function reduce(im, size) {
  clearGround(im);
  const {w, h, pixels: p} = im;
  let l = w, t = h, r = -1, b = -1;
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) if (p[(y * w + x) * 4 + 3] >= 128)
    [l, r, t, b] = [Math.min(l, x), Math.max(r, x), Math.min(t, y), Math.max(b, y)];
  if (r < 0) throw Error('the picture is empty');
  const sw = r - l + 1, sh = b - t + 1, k = Math.min(1, 29 / Math.max(sw, sh));
  const rw = Math.max(1, Math.round(sw * k)), rh = Math.max(1, Math.round(sh * k));
  const ox = (CELL - rw) >> 1, oy = (CELL - rh) >> 1, cell = Array(CELL * CELL).fill(null), counts = new Map();
  for (let y = 0; y < rh; y++) for (let x = 0; x < rw; x++) {
    const bins = new Map();
    for (let sy = t + Math.floor(y * sh / rh); sy < t + Math.floor((y + 1) * sh / rh); sy++)
      for (let sx = l + Math.floor(x * sw / rw); sx < l + Math.floor((x + 1) * sw / rw); sx++) {
        const i = (sy * w + sx) * 4;
        const bin = p[i + 3] < 128 ? '' : [p[i], p[i + 1], p[i + 2]].map(v => (v & ~31) + 16).join(',');
        bins.set(bin, (bins.get(bin) || 0) + 1);
      }
    const bin = [...bins].sort((a, b) => b[1] - a[1])[0]?.[0];
    if (bin) { cell[(y + oy) * CELL + x + ox] = bin; counts.set(bin, (counts.get(bin) || 0) + 1); }
  }
  if (!counts.size) throw Error('nothing of the picture survives at 33 px; use broader shapes that fill the frame');
  const palette = paletteOf(counts, size);
  return {cell: cell.map(c => c && nearest(c.split(',').map(Number), palette)), palette};
}

// The likeliest pair of eyes: two small dark patches side by side, each with a lighter face all
// round it. A stretch of dark outline is never one: it touches the clear ground.
function eyesIn(cell) {
  const at = (x, y) => x >= 0 && x < CELL && y >= 0 && y < CELL ? cell[y * CELL + x] : null;
  const dark = (x, y) => at(x, y) && lum(at(x, y)) < 100;
  const seen = new Set(), patches = [];
  for (let y = 0; y < CELL; y++) for (let x = 0; x < CELL; x++) {
    if (seen.has(y * CELL + x) || !dark(x, y)) continue;
    const group = [y * CELL + x];
    let enclosed = true;
    seen.add(group[0]);
    for (const j of group) {
      const gx = j % CELL, gy = Math.floor(j / CELL);
      for (const [nx, ny] of [[gx - 1, gy], [gx + 1, gy], [gx, gy - 1], [gx, gy + 1]]) {
        if (!dark(nx, ny)) enclosed &&= !!at(nx, ny);
        else if (!seen.has(ny * CELL + nx)) { seen.add(ny * CELL + nx); group.push(ny * CELL + nx); }
      }
    }
    const xs = group.map(v => v % CELL), ys = group.map(v => Math.floor(v / CELL));
    const [l, r, t, b] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    const e = {group, l, r, t, b, x: (l + r) / 2, y: (t + b) / 2};
    if (enclosed && group.length >= 2 && group.length <= 35 && e.r - e.l <= 6 && e.b - e.t <= 5) patches.push(e);
  }
  const pairs = patches.flatMap(a => patches.filter(b => b.x - a.x >= 5 && Math.abs(a.y - b.y) <= 2).map(b =>
    ({a, b, score: Math.abs(a.y - b.y) * 5 + Math.abs((a.x + b.x) / 2 - 16) + Math.abs(a.group.length - b.group.length)})));
  return pairs.sort((p, q) => p.score - q.score)[0];
}

// Blink, talk and sleep from the idle cell. A character closes its eyes and opens a mouth below
// them; an emblem glints (white or black when it has one colour), swells by a tenth, and dims to sleep.
function poses({cell: idle, palette}, kind) {
  const blink = idle.slice(), talk = idle.slice();
  const light = palette.reduce((a, b) => lum(a) > lum(b) ? a : b), dark = palette.reduce((a, b) => lum(a) < lum(b) ? a : b);
  if (kind === 'character') {
    const eyes = eyesIn(idle);
    if (!eyes) throw Error('no pair of eyes found; ask for two clear dark eyes on a lighter face, or make it an emblem');
    for (const eye of [eyes.a, eyes.b]) {
      const rim = new Map();
      for (const c of eye.group.flatMap(i => [idle[i - CELL], idle[i + CELL], idle[i - 1], idle[i + 1]]))
        if (c && lum(c) > 100) rim.set(c, (rim.get(c) || 0) + 1);
      const skin = [...rim].sort((a, b) => b[1] - a[1])[0]?.[0] ?? light;
      for (const i of eye.group) blink[i] = skin;
      for (let x = eye.l; x <= eye.r; x++) blink[eye.b * CELL + x] = dark;
    }
    const mx = Math.round((eyes.a.x + eyes.b.x) / 2), my = Math.min(29, Math.max(eyes.a.b, eyes.b.b) + 2);
    for (let y = my; y < my + 3; y++) for (let x = mx - 1; x <= mx + 1; x++) if (idle[y * CELL + x]) talk[y * CELL + x] = dark;
  } else if (kind === 'emblem') {
    const top = idle.findIndex(Boolean), [hi, lo] = light === dark ? [[255, 255, 255], [0, 0, 0]] : [light, dark];
    const glint = lum(idle[top]) > (lum(hi) + lum(lo)) / 2 ? lo : hi;
    for (const i of [top, top + 1, top + CELL, top + CELL + 1]) blink[i] = glint;
    for (let y = 0; y < CELL; y++) for (let x = 0; x < CELL; x++)
      talk[y * CELL + x] = idle[Math.round(16 + (y - 16) / 1.1) * CELL + Math.round(16 + (x - 16) / 1.1)];
  } else throw Error(USAGE);
  const sleep = (kind === 'character' ? blink : idle).map(c => c && c.map(v => Math.round(v * .55)));
  return [idle, blink, talk, sleep];
}

// The cells drawn side by side, `scale` times over, on `ground` with `gap` between them.
function draw(cells, scale = 1, gap = 0, ground = [0, 0, 0, 0]) {
  const w = (cells.length * (CELL + gap) + gap) * scale, h = (CELL + 2 * gap) * scale, pixels = Buffer.alloc(w * h * 4);
  for (let i = 0; i < w * h; i++) pixels.set(ground, i * 4);
  cells.forEach((cell, n) => cell.forEach((c, i) => {
    if (!c) return;
    const x0 = (gap + n * (CELL + gap) + i % CELL) * scale, y0 = (gap + Math.floor(i / CELL)) * scale;
    for (let y = y0; y < y0 + scale; y++) for (let x = x0; x < x0 + scale; x++) pixels.set([...c, 255], (y * w + x) * 4);
  }));
  return encode({w, h, pixels});
}

// The exact format Razor draws, read from the bytes as the server reads them.
function check(bytes) {
  if (bytes.length > 2048) throw Error(`the sheet is ${bytes.length} bytes, over 2048; ${SMALLER}`);
  const {w, h, type, pixels} = decode(bytes);
  if (w !== 4 * CELL || h !== CELL || type !== 6) throw Error(`the sheet is ${w} × ${h} of PNG colour type ${type}; it must be 132 × 33 RGBA`);
  const colours = new Set();
  for (let i = 0; i < w * h; i++) {
    if (pixels[i * 4 + 3] % 255) throw Error('the sheet has partly transparent pixels; alpha must be 0 or 255');
    if (pixels[i * 4 + 3]) colours.add(pixels.readUInt32BE(i * 4));
  }
  if (colours.size > 16) throw Error(`the sheet has ${colours.size} colours; it may have at most 16`);
  const cells = [0, 1, 2, 3].map(n => Buffer.concat(Array.from({length: CELL}, (_, y) =>
    pixels.subarray((y * w + n * CELL) * 4, (y * w + n * CELL + CELL) * 4))));
  if (cells.some(c => !c.some((v, i) => i % 4 === 3 && v))) throw Error('a cell of the sheet is empty');
  if (cells[1].equals(cells[0]) || cells[2].equals(cells[0]))
    throw Error('blink and talk must each differ from idle; use a larger, simpler subject');
  return {bytes: bytes.length, colours: colours.size};
}

function razor(...words) {
  const run = spawnSync('razor', words, {stdio: ['ignore', 'ignore', 'pipe'], encoding: 'utf8'});
  if (run.status !== 0) throw Error(`razor ${words.join(' ')} failed: ${run.stderr?.trim() || run.error?.message}`);
}

// Sets the avatar on a session through the server's local socket, with this home's device key,
// as docs/scripting.md describes.
async function wear(home, session, name) {
  const key = fs.readFileSync(path.join(home, 'device.key'), 'utf8').trim();
  return new Promise((resolve, reject) => {
    const request = http.request({socketPath: path.join(home, 'server.sock'), method: 'PUT',
      path: `/v1/sessions/${encodeURIComponent(session)}/avatar`,
      headers: {authorization: `Bearer ${key}`, 'content-type': 'text/plain'}}, response => {
      let body = '';
      response.on('data', d => body += d).on('end', () => response.statusCode < 300 ? resolve()
        : reject(Error(`the server refused the avatar for session ${session}: ${response.statusCode} ${body}`)));
    });
    request.on('error', reject);
    request.end(name);
  });
}

async function install(sheet, name, {pool = false, resident, session}) {
  if (!/^[A-Za-z0-9_-]{1,40}$/.test(name)) throw Error('an avatar name is 1 to 40 letters, digits, - or _');
  const bytes = read(sheet, 2048, SMALLER), home = process.env.RAZOR_HOME || path.join(os.homedir(), '.razor');
  check(bytes);
  if (resident) razor('config', 'get', `residents.${resident}`);
  // A new path every time, since a changed path is what tells the server the picture changed.
  const file = path.join(home, 'avatars', `${name}-${crypto.randomBytes(16).toString('hex')}.png`);
  fs.mkdirSync(path.dirname(file), {recursive: true});
  fs.writeFileSync(file, bytes, {flag: 'wx'});
  try { razor('config', 'set', `brand.avatars.${name}.file`, JSON.stringify(file)); } catch (e) { fs.rmSync(file); throw e; }
  razor('config', 'set', `brand.avatars.${name}.pool`, String(pool));
  if (resident) razor('config', 'set', `residents.${resident}.avatar`, JSON.stringify(name));
  // A missing key or socket means the server is not running; a refusal has no code.
  if (session) await wear(home, session, name).catch(e => {
    throw e.code ? Error(`Razor is not running, so avatar ${name} is installed but not yet worn: start Razor and type /avatar ${name} in the conversation`) : e;
  });
  return {file};
}

async function main([command, ...args]) {
  if (command === 'make' && args.length === 3) {
    const [picture, kind, sheet] = args;
    const frames = poses(reduce(decode(read(picture, 16 << 20, 'save it smaller, at most 16 MiB')), kind === 'emblem' ? 3 : 8), kind);
    const bytes = draw(frames), result = check(bytes), preview = sheet.replace(/(\.png)?$/i, '-preview.png');
    fs.mkdirSync(path.dirname(sheet), {recursive: true});
    fs.writeFileSync(sheet, bytes);
    fs.writeFileSync(preview, draw(frames, 8, 4, [21, 23, 28, 255]));
    return {sheet, preview, ...result};
  }
  if (command === 'check' && args.length === 1) return check(read(args[0], 2048, SMALLER));
  if (command === 'install') {
    const {values, positionals} = parseArgs({args, allowPositionals: true,
      options: {pool: {type: 'boolean'}, resident: {type: 'string'}, session: {type: 'string'}}});
    if (positionals.length === 2) return install(...positionals, values);
  }
  throw Error(USAGE);
}

main(process.argv.slice(2)).then(result => console.log(JSON.stringify(result)), e => {
  console.error(`avatar: ${e.message}`);
  process.exitCode = 1;
});
