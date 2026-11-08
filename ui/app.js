'use strict';
// Earshot UI. Runs inside Tauri (window.__TAURI__) or in a plain browser with sample data.
const tauri = window.__TAURI__;
const inv = (cmd, args) => (tauri ? tauri.core.invoke(cmd, args).catch(e => console.warn(cmd, e)) : Promise.resolve(null));
const $ = (s, r = document) => r.querySelector(s);
const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
const ISO = [31, 63, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];
const FS = 48000;

const ICONS = {
  eq: 'M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6',
  profiles: 'M6 11h4M8 9v4M15 12h.01M18 10h.01M17.3 5H6.7a4 4 0 0 0-4 3.6l-.9 7.2A3 3 0 0 0 4.8 19c.8 0 1.6-.4 2.1-1l1.9-2.3a2 2 0 0 1 1.5-.7h3.4a2 2 0 0 1 1.5.7l1.9 2.3a2.8 2.8 0 0 0 4.2-3.2l-1.6-7.2A4 4 0 0 0 17.3 5z',
  devices: 'M3 18v-6a9 9 0 0 1 18 0v6M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z',
  settings: 'M20 7h-9M14 17H5M17 20a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM7 10a3 3 0 1 0 0-6 3 3 0 0 0 0 6z',
  speaker: 'M11 5 6 9H2v6h4l5 4zM15.5 8.5a5 5 0 0 1 0 7M19 5a10 10 0 0 1 0 14',
  monitor: 'M2 4h20v12H2zM8 20h8M12 16v4',
  search: 'M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM21 21l-4.3-4.3',
  plus: 'M12 5v14M5 12h14',
  folder: 'M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
  refresh: 'M21 12a9 9 0 1 1-3-6.7L21 8M21 3v5h-5',
  bolt: 'M13 2 3 14h9l-1 8 10-12h-9z',
};
const ic = (n, s = 16) => `<svg class="ic" width="${s}" height="${s}" viewBox="0 0 24 24"><path d="${ICONS[n]}"/></svg>`;

// ---------- sample data (mirrors src-tauri/src/profiles.rs) ----------
const GAMES = [
  ['valorant', 'Valorant', 'VA', ['VALORANT-Win64-Shipping.exe'], [3150, 5, 5.5, 1]],
  ['cs2', 'Counter-Strike 2', 'CS', ['cs2.exe'], [2400, 4.5, 4, 1.5]],
  ['apex', 'Apex Legends', 'AL', ['r5apex.exe', 'r5apex_dx12.exe'], [2000, 5.5, 6, 0.5]],
  ['fortnite', 'Fortnite', 'FN', ['FortniteClient-Win64-Shipping.exe'], [2800, 4, 5, 1]],
  ['pubg', 'PUBG: Battlegrounds', 'PB', ['TslGame.exe'], [1800, 5, 4.5, 1]],
  ['r6', 'Rainbow Six Siege', 'R6', ['RainbowSix.exe', 'RainbowSix_Vulkan.exe'], [2200, 5, 6.5, 0.5]],
  ['warzone', 'Call of Duty: Warzone', 'WZ', ['cod.exe', 'ModernWarfare.exe'], [2600, 5.5, 6, 1]],
  ['tarkov', 'Escape from Tarkov', 'EF', ['EscapeFromTarkov.exe'], [1600, 4.5, 3.5, 2]],
  ['overwatch', 'Overwatch 2', 'OW', ['Overwatch.exe'], [3000, 3.5, 3, 1]],
  ['finals', 'The Finals', 'TF', ['Discovery.exe'], [2500, 4.5, 6.5, 0.5]],
].map(([id, name, monogram, exes, t]) => ({ id, name, monogram, exes: exes.map(e => e.toLowerCase()), tuning: { presenceHz: t[0], presenceDb: t[1], rumbleCutDb: t[2], airDb: t[3] } }));

const B = (kind, freq, gainDb, q) => ({ kind, freq, gainDb: clamp(gainDb, -15, 15), q, enabled: true });
const flat = () => ISO.map((f, i) => B(i === 0 ? 'lowShelf' : i === 9 ? 'highShelf' : 'peaking', f, 0, i === 0 || i === 9 ? 0.71 : 1.41));
function footsteps(t) {
  const c = t.rumbleCutDb, p = t.presenceDb, hz = clamp(t.presenceHz, 1500, 4000);
  return [B('lowShelf', 31, -0.4 * c, 0.71), B('peaking', 63, -0.6 * c, 1.2), B('peaking', 125, -c, 1.1), B('peaking', 220, -0.75 * c, 1.3),
    B('peaking', 500, -0.2 * c, 1.41), B('peaking', 1000, 0.15 * p, 1.41), B('peaking', hz, p, 1.4), B('peaking', Math.min(hz * 1.55, 6000), 0.8 * p, 1.8),
    B('peaking', 8000, t.airDb, 1.41), B('highShelf', 16000, t.airDb - 2, 0.71)];
}
const NOW = 1794200000;
const AGO = [42, 3, 1380, 4320, 0, 2900, 9800, 610, 0, 15200];
function mockSnapshot() {
  const profiles = [{ id: 'flat', name: 'Flat (desktop)', gameId: null, preset: 'flat', bands: flat(), auto: false, lastSeenUnix: null },
    ...GAMES.map((g, i) => ({ id: g.id, name: g.name, gameId: g.id, preset: 'footstepsPlus', bands: footsteps(g.tuning), auto: !['fortnite', 'overwatch'].includes(g.id), lastSeenUnix: AGO[i] ? NOW - AGO[i] * 60 : null }))];
  const ow = profiles.find(p => p.id === 'overwatch'), ef = profiles.find(p => p.id === 'tarkov');
  Object.assign(ow, { preset: 'flat', bands: flat() });
  ef.preset = 'custom'; ef.bands[6].gainDb = 6; ef.bands[2].gainDb = -2;
  return {
    games: GAMES, profiles, activeId: 'cs2', bypass: false,
    detected: { gameId: 'cs2', gameName: 'Counter-Strike 2', exe: 'cs2.exe', sinceUnix: NOW - 1260 },
    settings: { autoSwitch: true, pollMs: 1500, revertOnExit: true, fallbackProfile: 'flat', headroomDb: 0.5, loudnessGuard: true, systemWide: true, bypassHotkey: 'Ctrl+Alt+B', startWithWindows: true, minimizeToTray: true, outputDevice: null },
    apo: { installed: true, version: '1.3.2', installPath: 'C:\\Program Files\\EqualizerAPO', configPath: 'C:\\Program Files\\EqualizerAPO\\config', blockPresent: true },
  };
}
const MOCK_DEVICES = [
  ['{0.0.0.00000000}.{6a1f}', 'Headphones', 'Arctis Nova 7', true, true],
  ['{0.0.0.00000000}.{91c4}', 'Speakers', 'Realtek(R) Audio', true, true],
  ['{0.0.0.00000000}.{2be0}', 'Headset Earphone', 'HyperX Cloud II Wireless', true, false],
  ['{0.0.0.00000000}.{f7d2}', 'LG ULTRAGEAR', 'NVIDIA High Definition Audio', false, false],
].map(([id, name, adapter, active, apoAttached]) => ({ id, name, adapter, active, apoAttached }));

// ---------- DSP (same RBJ formulas as src-tauri/src/dsp.rs) ----------
function coeffs(b) {
  const w = 2 * Math.PI * Math.min(b.freq, FS * 0.49) / FS, c = Math.cos(w), al = Math.sin(w) / (2 * b.q), A = 10 ** (b.gainDb / 40), sq = 2 * Math.sqrt(A) * al;
  const r = b.kind === 'lowShelf' ? [A * ((A + 1) - (A - 1) * c + sq), 2 * A * ((A - 1) - (A + 1) * c), A * ((A + 1) - (A - 1) * c - sq), (A + 1) + (A - 1) * c + sq, -2 * ((A - 1) + (A + 1) * c), (A + 1) + (A - 1) * c - sq]
    : b.kind === 'highShelf' ? [A * ((A + 1) + (A - 1) * c + sq), -2 * A * ((A - 1) + (A + 1) * c), A * ((A + 1) + (A - 1) * c - sq), (A + 1) - (A - 1) * c + sq, 2 * ((A - 1) - (A + 1) * c), (A + 1) - (A - 1) * c - sq]
    : [1 + al * A, -2 * c, 1 - al * A, 1 + al / A, -2 * c, 1 - al / A];
  return r.map(v => v / r[3]);
}
function magDb(k, f) {
  const w = 2 * Math.PI * f / FS, c1 = Math.cos(w), s1 = Math.sin(w), c2 = Math.cos(2 * w), s2 = Math.sin(2 * w);
  const nr = k[0] + k[1] * c1 + k[2] * c2, ni = -(k[1] * s1 + k[2] * s2), dr = 1 + k[4] * c1 + k[5] * c2, di = -(k[4] * s1 + k[5] * s2);
  return 10 * Math.log10((nr * nr + ni * ni) / (dr * dr + di * di));
}
const logF = (i, n) => 20 * 1000 ** (i / (n - 1));
function response(bands, freqs) {
  const ks = bands.filter(b => b.enabled).map(coeffs);
  return freqs.map(f => ks.reduce((s, k) => s + magDb(k, f), 0));
}
function guard(bands) {
  const fr = Array.from({ length: 400 }, (_, i) => logF(i, 400)), db = response(bands, fr);
  let pk = -99, ph = 20;
  db.forEach((d, i) => { if (d > pk) { pk = d; ph = fr[i]; } });
  const mb = Math.max(0, ...bands.map(b => b.gainDb)), boost = Math.max(pk, mb, 0), st = S.snap.settings;
  return { peakDb: pk, peakHz: ph, preampDb: st.loudnessGuard && boost > 0 ? -(boost + st.headroomDb) : 0 };
}
// Sample analyzer signal: ambience + footsteps at 2.1 steps/s + an explosion every 7 s (pure function of t).
function synth(t) {
  const step = Math.exp(-((t * 2.1) % 1) * 14), bp = (t / 7) % 1, boom = bp < 0.12 ? Math.exp(-bp * 30) : 0;
  const db = SPEC_F.map((f, i) => {
    const o = Math.log2(f / 1000);
    return -40 - 3 * o - 0.6 * o * o + 2.2 * Math.sin(i * 1.9 + t * 7.3) + 1.6 * Math.sin(i * 0.53 - t * 4.1) + 1.1 * Math.sin(i * 3.7 + t * 11)
      + 22 * step * Math.exp(-((Math.log2(f / 2900) / 0.55) ** 2)) + 26 * boom * Math.exp(-((Math.log2(f / 90) / 1.2) ** 2));
  });
  return { db, step };
}
const SPEC_F = Array.from({ length: 96 }, (_, i) => logF(i, 96));

// ---------- formatting ----------
const fDb = (d, p = 1) => (d > 0.05 ? '+' : d < -0.05 ? '\u2212' : '') + Math.abs(d).toFixed(p);
const fHz = f => (f >= 1000 ? (f / 1000).toFixed(f >= 10000 ? 1 : 2) + ' kHz' : Math.round(f) + ' Hz');
const fShort = f => (f >= 995 ? +(f / 1000).toFixed(1) + 'k' : String(Math.round(f)));
const KIND = { peaking: 'PK', lowShelf: 'LS', highShelf: 'HS', highPass: 'HP', lowPass: 'LP' };
const APO_KIND = { peaking: 'PK', lowShelf: 'LSC', highShelf: 'HSC', highPass: 'HPQ', lowPass: 'LPQ' };
const PRESET = { flat: 'Flat', footstepsPlus: 'Footsteps+', custom: 'Custom' };
function ago(u) {
  if (!u) return '\u2014';
  const m = Math.round((S.now - u) / 60);
  return m < 5 ? 'Now' : m < 60 ? m + ' min ago' : m < 1440 ? Math.round(m / 60) + ' h ago' : m < 2880 ? 'Yesterday' : Math.round(m / 1440) + ' days ago';
}

// ---------- state ----------
const S = { snap: mockSnapshot(), devices: MOCK_DEVICES, sel: 6, t: 1.92, view: null, flash: 0, now: NOW, cfg: '' };
const active = () => S.snap.profiles.find(p => p.id === S.snap.activeId) || S.snap.profiles[0];
const bands = () => S.view || active().bands;
const gameOf = p => S.snap.games.find(g => g.id === p.gameId);

// ---------- chrome ----------
function renderCapsule() {
  const s = S.snap, d = s.detected, p = active(), el = $('#capsule');
  const parts = s.bypass ? ['A/B bypass', 'Ctrl+Alt+B', 'EQ off'] : d ? ['Auto-switch', d.exe + ' detected', PRESET[p.preset] + ' active']
    : ['Auto-switch', 'watching ' + s.profiles.filter(x => x.auto).length + ' games', PRESET[p.preset] + ' \u00b7 ' + p.name];
  el.className = 'capsule' + (d && !s.bypass ? ' on' : '') + (S.flash ? ' flash' : '');
  el.innerHTML = `<span class="dot"></span><span class="k">${parts[0]}</span><span class="sep">\u00b7</span><span class="exe">${parts[1]}</span><span class="sep">\u00b7</span><span class="act">${parts[2]}</span>`;
  $('#ab').classList.toggle('byp', s.bypass);
}
function renderNav() {
  const r = route();
  $('#nav').innerHTML = [['eq', 'Equalizer', 1], ['profiles', 'Profiles', 2], ['devices', 'Devices', 3], ['settings', 'Settings', 4]]
    .map(([id, n, k]) => `<a href="#${id}" class="${r === id ? 'on' : ''}">${ic(id)}${n}<kbd>${k}</kbd></a>`).join('');
}
function renderGames() {
  const s = S.snap;
  $('#games').innerHTML = s.profiles.map(p => {
    const g = gameOf(p), live = s.detected && s.detected.gameId === p.id;
    return `<div class="gr${p.id === s.activeId ? ' act' : ''}" data-id="${p.id}"><span class="mg">${g ? g.monogram : ic('monitor', 14)}</span>
      <div class="gi"><b>${p.name}</b><small>${g ? g.exes[0] : 'Fallback when no game runs'}</small></div>${live ? '<span class="live">LIVE</span>' : ''}
      ${g ? `<span class="tg${p.auto ? ' on' : ''}" data-auto="${p.id}"></span>` : ''}</div>`;
  }).join('');
  const dev = S.devices.find(d => d.id === s.settings.outputDevice) || S.devices[0];
  $('#outcard').innerHTML = `<div class="lbl">Output</div><div class="r">${ic('devices', 14)}<b>${dev.name} (${dev.adapter})</b></div>
    <div class="r"><span class="led ${s.apo.installed ? 'on' : ''}"></span>Equalizer APO ${s.apo.version || 'not found'}<span style="margin-left:auto" class="mono">${s.settings.systemWide ? 'system-wide' : 'off'}</span></div>`;
}

// ---------- EQ screen ----------
const G = {};
function buildGraph() {
  const el = $('#graph'), W = el.clientWidth, H = el.clientHeight;
  if (!W) return;
  const p = { l: 42, r: 46, t: 34, b: 24 }, w = W - p.l - p.r, h = H - p.t - p.b;
  Object.assign(G, { W, H, p, w, h, X: f => p.l + Math.log(f / 20) / Math.log(1000) * w, Y: d => p.t + (15 - d) / 30 * h, SY: d => p.t + (-14 - d) / 76 * h });
  let s = `<svg width="${W}" height="${H}"><defs><linearGradient id="cg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#FFD60A" stop-opacity=".22"/><stop offset="1" stop-color="#FFD60A" stop-opacity="0"/></linearGradient></defs>`;
  const x2 = G.X(2000), x4 = G.X(4000);
  s += `<rect id="fb" class="fb" x="${x2}" y="${p.t}" width="${x4 - x2}" height="${h}" opacity=".05"/><text class="ax fbl" x="${(x2 + x4) / 2}" y="${p.t - 8}" text-anchor="middle">FOOTSTEP BAND</text>`;
  for (const f of [20, 50, 100, 200, 500, 1e3, 2e3, 5e3, 1e4, 2e4]) {
    const x = Math.round(G.X(f)) + 0.5;
    s += `<line class="gl" x1="${x}" x2="${x}" y1="${p.t}" y2="${p.t + h}"/><text class="ax" x="${x}" y="${H - 7}" text-anchor="middle">${f >= 1e3 ? f / 1e3 + 'k' : f}</text>`;
  }
  for (const f of [30, 40, 60, 70, 80, 90, 300, 400, 600, 700, 800, 900, 3e3, 4e3, 6e3, 7e3, 8e3, 9e3]) { const x = Math.round(G.X(f)) + 0.5; s += `<line class="gl" x1="${x}" x2="${x}" y1="${p.t}" y2="${p.t + h}" opacity=".45"/>`; }
  for (const d of [12, 6, 0, -6, -12]) {
    const y = Math.round(G.Y(d)) + 0.5;
    s += `<line class="gl${d ? '' : ' z'}" x1="${p.l}" x2="${p.l + w}" y1="${y}" y2="${y}"/><text class="ax" x="${p.l - 8}" y="${y + 3.5}" text-anchor="end">${d > 0 ? '+' + d : d}</text>`;
  }
  for (const d of [-20, -40, -60, -80]) s += `<text class="ax" x="${p.l + w + 8}" y="${G.SY(d) + 3.5}" opacity=".7">${d}</text>`;
  s += `<text class="gtitle" x="${p.l}" y="${p.t - 14}">PARAMETRIC EQ \u00b7 dB</text><text class="gtitle" x="${p.l + w + 46}" y="${p.t - 14}" text-anchor="end">ANALYZER \u00b7 dBFS</text>`;
  s += `<path id="spec" class="spec"/><path id="specl" class="specl"/><g id="cg2"><path id="cfill" class="cfill"/><path id="cline" class="cline"/></g><g id="nodes"></g><g id="tip" class="tip"></g></svg>`;
  el.innerHTML = s;
  G.curveF = Array.from({ length: 260 }, (_, i) => logF(i, 260));
  drawCurve();
  drawSpectrum();
}
function drawCurve() {
  if (!G.W) return;
  const bs = bands(), db = response(bs, G.curveF), Y0 = G.Y(0);
  const pts = G.curveF.map((f, i) => `${G.X(f).toFixed(1)},${G.Y(clamp(db[i], -15, 15)).toFixed(1)}`);
  $('#cline').setAttribute('d', 'M' + pts.join('L'));
  $('#cfill').setAttribute('d', `M${G.p.l},${Y0}L${pts.join('L')}L${G.p.l + G.w},${Y0}Z`);
  $('#cg2').setAttribute('class', S.snap.bypass ? 'byp' : '');
  $('#nodes').innerHTML = bs.map((b, i) => `<g class="node${i === S.sel ? ' sel' : ''}" data-i="${i}" transform="translate(${G.X(b.freq).toFixed(1)},${G.Y(b.gainDb).toFixed(1)})"><circle r="8"/><text y="3.3">${i + 1}</text></g>`).join('');
  const b = bs[S.sel];
  if (b) {
    const x = G.X(b.freq), y = G.Y(b.gainDb), txt = `${fHz(b.freq)}  ${fDb(b.gainDb)} dB  Q ${b.q.toFixed(2)}`, tw = txt.length * 6.8 + 16;
    const tx = clamp(x - tw / 2, G.p.l + 2, G.p.l + G.w - tw - 2), ty = y > G.p.t + 50 ? y - 40 : y + 18;
    $('#tip').innerHTML = `<line class="vl" x1="${x}" x2="${x}" y1="${G.p.t}" y2="${G.p.t + G.h}"/><rect x="${tx}" y="${ty}" width="${tw}" height="22" rx="4"/><text x="${tx + 8}" y="${ty + 15}" xml:space="preserve">${txt}</text>`;
  }
  G.eqAtSpec = S.snap.bypass ? SPEC_F.map(() => 0) : response(bs, SPEC_F);
  renderReadouts();
}
function drawSpectrum(frame) {
  if (!G.W) return;
  const fr = frame || synth(S.t), eq = G.eqAtSpec || [];
  const pts = SPEC_F.map((f, i) => `${G.X(f).toFixed(1)},${clamp(G.SY(fr.db[i] + (eq[i] || 0)), G.p.t, G.p.t + G.h).toFixed(1)}`);
  const base = G.p.t + G.h;
  $('#specl').setAttribute('d', 'M' + pts.join('L'));
  $('#spec').setAttribute('d', `M${G.X(20)},${base}L${pts.join('L')}L${G.X(20000)},${base}Z`);
  $('#fb').setAttribute('opacity', (0.04 + 0.16 * fr.step).toFixed(3));
  S.step = fr.step;
  const led = $('#steplit');
  if (led) { led.closest('.ro').classList.toggle('lit', fr.step > 0.45); led.className = 'led' + (fr.step > 0.45 ? ' y' : ''); $('#stepv').textContent = (-34 + 22 * fr.step).toFixed(1); }
}
function renderReadouts() {
  const g = guard(bands()), b = bands()[S.sel] || bands()[0];
  $('#readouts').innerHTML = [
    ['Preamp \u00b7 guard', `${fDb(S.snap.bypass ? 0 : g.preampDb)}<small>dB</small>`],
    ['Curve peak', g.peakDb > 0.05 ? `${fDb(g.peakDb)}<small>@ ${fShort(g.peakHz)}</small>` : '0.0<small>flat</small>'],
    [`Band ${S.sel + 1} \u00b7 ${KIND[b.kind]}`, `${fShort(b.freq)}<small>Hz</small> ${fDb(b.gainDb)}<small>dB</small>`],
    ['<span id="steplit" class="led"></span>Step detect', `<span id="stepv">-34.0</span><small>dB</small>`],
  ].map(([l, v]) => `<div class="ro"><div class="lbl">${l}</div><div class="num">${v}</div></div>`).join('');
}
function renderStrip() {
  const bs = bands(), g = guard(bs), pos = d => (15 - clamp(d, -15, 15)) / 30 * 100;
  const fill = d => { const a = pos(d), z = 50; return `top:${Math.min(a, z)}%;height:${Math.abs(a - z)}%`; };
  $('#strip').innerHTML = `<div class="scale">${[15, 6, 0, -6, -15].map(d => `<span style="top:${pos(d)}%">${fDb(d, 0)}</span>`).join('')}</div>` + bs.map((b, i) =>
    `<div class="ch${i === S.sel ? ' sel' : ''}" data-i="${i}"><div class="chh"><b>${fShort(b.freq)}</b><i>${KIND[b.kind]}</i></div><div class="trk"><div class="fill" style="${fill(b.gainDb)}"></div><div class="cap" style="top:${pos(b.gainDb)}%"></div></div><div class="chv">${fDb(b.gainDb)}</div><div class="chq">Q ${b.q.toFixed(2)}</div></div>`).join('') +
    `<div class="ch pre"><div class="chh"><b>PRE</b><i>${S.snap.settings.loudnessGuard ? 'AUTO' : 'MAN'}</i></div><div class="trk"><div class="fill" style="${fill(g.preampDb)}"></div><div class="cap" style="top:${pos(g.preampDb)}%"></div></div><div class="chv">${fDb(g.preampDb)}</div><div class="chq">no-clip</div></div>`;
}
function renderEQ() {
  const p = active(), g = gameOf(p);
  $('#pname').textContent = g ? g.name : p.name;
  const cur = S.viewPreset || p.preset;
  $('#presets').innerHTML = ['flat', 'footstepsPlus', 'custom'].map(k => `<button data-preset="${k}" class="${cur === k ? 'on' + (k === 'footstepsPlus' ? ' y' : '') : ''}">${PRESET[k]}</button>`).join('');
  if (!G.W) buildGraph(); else drawCurve();
  renderStrip();
}

// ---------- other screens ----------
function thumb(bs) {
  const fr = Array.from({ length: 40 }, (_, i) => logF(i, 40)), db = response(bs, fr);
  return `<svg class="thumb" width="112" height="26"><line x1="0" x2="112" y1="13.5" y2="13.5"/><path d="M${fr.map((f, i) => `${(i / 39 * 112).toFixed(1)},${(13 - db[i] * 1.3).toFixed(1)}`).join('L')}"/></svg>`;
}
function renderProfiles() {
  const s = S.snap;
  $('#s-profiles').innerHTML = `<div class="toolbar"><div><div class="lbl">Library</div><h1>Game profiles</h1></div><span class="sub" style="margin:6px 0 0 6px">${s.profiles.length - 1} shooters \u00b7 ${s.profiles.filter(p => p.auto).length} auto-switch</span>
    <div class="search">${ic('search', 14)}Search games or .exe</div><button class="btn">${ic('plus', 14)}Custom game</button></div>
    <div class="panel scroll" style="flex:1;min-height:0"><table><thead><tr><th>Game</th><th>Process</th><th>Preset</th><th>Curve</th><th>Presence</th><th>Rumble cut</th><th>Last detected</th><th>Auto</th></tr></thead><tbody>${
    s.profiles.filter(p => p.gameId).map(p => {
      const g = gameOf(p), live = s.detected && s.detected.gameId === p.id;
      return `<tr><td><div class="gcell"><span class="mg">${g.monogram}</span>${p.name}${live ? ' <span class="live">LIVE</span>' : ''}</div></td><td class="m">${g.exes[0]}${g.exes.length > 1 ? ` <span class="tag">+${g.exes.length - 1}</span>` : ''}</td>
      <td><span class="tag${p.preset === 'footstepsPlus' ? ' y' : ''}">${PRESET[p.preset]}</span></td><td>${thumb(p.bands)}</td><td class="m">${fShort(g.tuning.presenceHz)} ${fDb(g.tuning.presenceDb)}</td>
      <td class="m">${fDb(-g.tuning.rumbleCutDb)} dB</td><td class="m">${live ? 'Now' : ago(p.lastSeenUnix)}</td><td><span class="tg${p.auto ? ' on' : ''}" data-auto="${p.id}"></span></td></tr>`;
    }).join('')}</tbody></table></div>`;
}
function apoText() {
  const p = active(), g = guard(p.bands), byp = S.snap.bypass, hz = f => (f >= 100 ? Math.round(f) : +f.toFixed(1));
  const dev = S.devices.find(d => d.id === S.snap.settings.outputDevice) || S.devices[0];
  return ['# --- Earshot begin (generated, edits inside are overwritten) ---', `# Profile: ${PRESET[p.preset]} / ${p.name}`, `Device: ${dev.name} ${dev.adapter}`, `Preamp: ${(byp ? 0 : g.preampDb).toFixed(1)} dB`,
    ...p.bands.map((b, i) => `Filter ${i + 1}: ${byp ? 'OFF' : 'ON'} ${APO_KIND[b.kind]} Fc ${hz(b.freq)} Hz Gain ${b.gainDb.toFixed(1)} dB Q ${b.q.toFixed(2)}`), 'Device: all', '# --- Earshot end ---'].join('\n');
}
const hl = t => t.split('\n').map(l => l.startsWith('#') ? `<span class="c">${l}</span>` : l.replace(/^(\w+(?: \d+)?:)/, '<span class="k">$1</span>').replace(/ (ON) /, ' <span class="on">$1</span> ').replace(/(-?\d+(?:\.\d+)?)( Hz| dB)/g, '<span class="v">$1</span>$2')).join('\n');
function renderDevices() {
  const s = S.snap, a = s.apo, sel = s.settings.outputDevice || S.devices[0].id;
  $('#s-devices').innerHTML = `<div class="toolbar"><div><div class="lbl">Audio</div><h1>Output &amp; Equalizer APO</h1></div><button class="btn" style="margin-left:auto" data-act="refresh">${ic('refresh', 14)}Rescan</button><button class="btn pri" data-act="apply">${ic('bolt', 14)}Apply now</button></div>
  <div class="cols"><div class="panel card"><div class="lbl">Playback devices</div>${S.devices.map(d => `<div class="dev${d.id === sel ? ' on' : ''}" data-dev="${d.id}"><span class="di">${ic(/speak/i.test(d.name) ? 'speaker' : /LG|monitor/i.test(d.name) ? 'monitor' : 'devices', 18)}</span>
    <div class="gi"><b>${d.name}</b><small>${d.adapter}</small></div>${d.id === sel ? '<span class="tag y">EARSHOT</span>' : ''}<span class="tag${d.apoAttached ? ' g' : ''}">${d.apoAttached ? 'APO linked' : 'APO off'}</span>${d.active ? '' : '<span class="tag">Unplugged</span>'}</div>`).join('')}
    <div class="lbl" style="margin-top:6px">Equalizer APO</div><div class="kv"><span>Status</span><b><span class="led ${a.installed ? 'on' : ''}" style="display:inline-block;margin-right:7px"></span>${a.installed ? 'Installed \u00b7 v' + a.version : 'Not installed'}</b>
    <span>Install path</span><b class="mono">${a.installPath || '\u2014'}</b><span>Config folder</span><b class="mono">${a.configPath || '\u2014'}</b><span>Earshot block</span><b>${a.blockPresent ? 'Present in config.txt' : 'Not written yet'}</b><span>Write mode</span><b>${s.settings.systemWide ? 'On every profile switch' : 'Manual'}</b></div></div>
  <div class="panel card"><div class="toolbar"><div class="lbl">config.txt \u00b7 managed block</div><span class="tag g" style="margin-left:auto">Live reload</span></div><pre>${hl(S.cfg || apoText())}</pre>
    <div class="lbl">Recent writes</div><div class="log">${LOG.map(([t, m, d]) => `<div><span class="mono">${t}</span><b>${m}</b><span class="mono">${d}</span></div>`).join('')}</div></div></div>`;
}
const LOG = [['21:14:03', 'cs2.exe detected → Footsteps+ / Counter-Strike 2', '14 lines · 3 ms'], ['20:52:41', 'A/B bypass off (Ctrl+Alt+B)', '14 lines · 2 ms'],
  ['20:52:37', 'A/B bypass on (Ctrl+Alt+B)', '14 lines · 2 ms'], ['20:31:10', 'Band 7 edited: 2.40 kHz +4.5 dB', '14 lines · 3 ms'], ['19:58:22', 'Valorant exited → Flat (desktop)', '14 lines · 2 ms']];
const tg = (k, on) => `<span class="tg${on ? ' on' : ''}" data-set="${k}"></span>`;
function renderSettings() {
  const st = S.snap.settings, row = (t, d, ctl) => `<div class="row"><div class="gi"><b>${t}</b><small>${d}</small></div>${ctl}</div>`;
  const grp = (t, rows) => `<div class="grp"><div class="lbl">${t}</div><div class="panel">${rows.join('')}</div></div>`;
  const p = active();
  const tray = `<div class="panel card tray"><div class="lbl">Tray icon · preview</div><div class="flyout"><div class="mi dis"><span class="led ${S.snap.bypass ? '' : 'y'}"></span>${S.snap.bypass ? 'Bypassed (A/B)' : PRESET[p.preset]} — ${p.name}</div><hr>
    <div class="mi">A/B bypass<kbd>${st.bypassHotkey}</kbd></div><div class="mi">Open Earshot</div><div class="mi">Quit</div></div>
    <div class="taskbar"><span class="tip2">Earshot — ${PRESET[p.preset]} · ${p.name}</span>${$('.logo').outerHTML}${ic('speaker', 15)}<span class="mono">21:14</span></div>
    <small class="note">Left-click opens the window. The label follows the active profile and the bypass state.</small></div>`;
  $('#s-settings').innerHTML = `<div class="toolbar"><div><div class="lbl">Preferences</div><h1>Settings</h1></div></div><div class="setwrap"><div class="set scroll">${[
    grp('Auto-activate', [row('Switch profiles when a game starts', 'Watches the process list for the executables of enabled games.', tg('autoSwitch', st.autoSwitch)),
      row('Check interval', 'How often the process list is read. 1.5 s costs well under 0.1% CPU.', `<div class="seg">${[500, 1500, 3000].map(v => `<button data-poll="${v}" class="${st.pollMs === v ? 'on' : ''}">${v / 1000} s</button>`).join('')}</div>`),
      row('Revert to Flat when the game exits', 'Desktop audio, music and calls go back to an untouched signal.', tg('revertOnExit', st.revertOnExit))]),
    grp('Loudness', [row('No-clip loudness guard', 'Lowers the preamp by the curve peak so boosted bands never clip.', tg('loudnessGuard', st.loudnessGuard)),
      row('Safety headroom', 'Extra margin on top of the computed peak.', `<div class="slider"><i style="width:${st.headroomDb / 3 * 100}%"></i></div><span class="val">${st.headroomDb.toFixed(1)} dB</span>`)]),
    grp('System-wide', [row('Apply through Equalizer APO', 'Writes a managed block into config.txt; every app and game hears the curve.', tg('systemWide', st.systemWide)),
      row('A/B bypass hotkey', 'Global shortcut, works while the game has focus.', `<div class="keys">${st.bypassHotkey.split('+').map(k => `<kbd>${k}</kbd>`).join('')}</div>`)]),
    grp('Startup', [row('Start with Windows', 'Launches minimized to the tray.', tg('startWithWindows', st.startWithWindows)),
      row('Close button hides to tray', 'Tray icon shows the active profile and the bypass state.', tg('minimizeToTray', st.minimizeToTray))]),
  ].join('')}</div>${tray}</div>`;
}

// ---------- routing + render ----------
const SCREENS = ['eq', 'profiles', 'devices', 'settings'];
const route = () => { const h = location.hash.slice(1); return SCREENS.includes(h) ? h : 'eq'; };
function render() {
  const r = route();
  SCREENS.forEach(s => $('#s-' + s).classList.toggle('on', s === r));
  renderNav(); renderCapsule(); renderGames();
  ({ eq: renderEQ, profiles: renderProfiles, devices: renderDevices, settings: renderSettings })[r]();
}
window.addEventListener('hashchange', () => { G.W = 0; render(); });
window.addEventListener('resize', () => { if (route() === 'eq') buildGraph(); });

// ---------- interaction ----------
function setSnap(s) { if (s) { S.snap = s; S.view = null; } render(); }
document.addEventListener('click', e => {
  const t = e.target.closest('[data-auto],[data-id],[data-preset],[data-set],[data-poll],[data-dev],[data-act],[data-win],#ab,.ch[data-i]');
  if (!t) return;
  const d = t.dataset, s = S.snap;
  if (d.auto) { const p = s.profiles.find(x => x.id === d.auto); p.auto = !p.auto; inv('set_auto', { profileId: p.id, enabled: p.auto }).then(setSnap); render(); }
  else if (d.id) { s.activeId = d.id; inv('activate_profile', { profileId: d.id }).then(setSnap); render(); }
  else if (d.preset) {
    const p = active(), g = gameOf(p);
    if (d.preset !== 'custom') { p.bands = d.preset === 'flat' ? flat() : footsteps(g ? g.tuning : GAMES[1].tuning); p.preset = d.preset; }
    inv('set_preset', { profileId: p.id, preset: d.preset }).then(setSnap); render();
  }
  else if (d.set) { s.settings[d.set] = !s.settings[d.set]; inv('update_settings', { settings: s.settings }).then(setSnap); render(); }
  else if (d.poll) { s.settings.pollMs = +d.poll; inv('update_settings', { settings: s.settings }).then(setSnap); render(); }
  else if (d.dev) { s.settings.outputDevice = d.dev; inv('update_settings', { settings: s.settings }).then(setSnap); render(); }
  else if (d.act === 'apply') inv('apply_now').then(r => { if (r) S.cfg = r.text; render(); });
  else if (d.act === 'refresh') inv('list_devices').then(r => { if (r) S.devices = r; render(); });
  else if (d.win) { if (tauri) tauri.window.getCurrentWindow()[d.win](); }
  else if (t.id === 'ab') { s.bypass = !s.bypass; inv('toggle_bypass'); render(); }
  else if (d.i) { S.sel = +d.i; render(); }
});
// Drag band nodes (freq + gain), wheel for Q; drag faders (gain).
let drag = null;
document.addEventListener('pointerdown', e => {
  const n = e.target.closest('.node'), ch = e.target.closest('.ch[data-i] .trk');
  if (n) { S.sel = +n.dataset.i; drag = { kind: 'node' }; }
  else if (ch) { S.sel = +ch.parentNode.dataset.i; drag = { kind: 'fader', el: ch }; }
  else return;
  e.preventDefault(); onDrag(e);
});
function onDrag(e) {
  if (!drag) return;
  const b = active().bands[S.sel];
  if (drag.kind === 'node') {
    const r = $('#graph').getBoundingClientRect(), x = e.clientX - r.left, y = e.clientY - r.top;
    b.freq = clamp(20 * 1000 ** ((x - G.p.l) / G.w), 20, 20000);
    b.gainDb = Math.round(clamp(15 - (y - G.p.t) / G.h * 30, -15, 15) * 10) / 10;
  } else {
    const r = drag.el.getBoundingClientRect();
    b.gainDb = Math.round(clamp(15 - (e.clientY - r.top) / r.height * 30, -15, 15) * 10) / 10;
  }
  active().preset = 'custom';
  inv('set_band', { profileId: active().id, index: S.sel, band: b });
  renderEQ();
}
document.addEventListener('pointermove', onDrag);
document.addEventListener('pointerup', () => { if (drag) { drag = null; inv('commit_edit').then(setSnap); } });
$('#graph').addEventListener('wheel', e => {
  const b = active().bands[S.sel]; if (!b) return;
  e.preventDefault(); b.q = Math.round(clamp(b.q * (e.deltaY < 0 ? 1.08 : 1 / 1.08), 0.1, 18) * 100) / 100;
  inv('set_band', { profileId: active().id, index: S.sel, band: b }); drawCurve(); renderStrip();
}, { passive: false });
document.addEventListener('keydown', e => { const k = +e.key; if (k >= 1 && k <= 4 && !e.ctrlKey) location.hash = SCREENS[k - 1]; });

// ---------- boot ----------
async function boot() {
  if (tauri) {
    S.now = Date.now() / 1000;
    const [snap, devs] = await Promise.all([inv('get_snapshot'), inv('list_devices')]);
    if (snap) S.snap = snap;
    if (devs && devs.length) S.devices = devs;
    tauri.event.listen('earshot:snapshot', ev => setSnap(ev.payload));
    tauri.event.listen('earshot:detected', () => { S.flash = 1; renderCapsule(); setTimeout(() => { S.flash = 0; renderCapsule(); }, 2200); });
  }
  render();
  if (window.__CAPTURE__) return; // capture tool drives frames via window.__seek
  let last = performance.now();
  const tick = async now => {
    const dt = now - last;
    if (dt > 33 && route() === 'eq') {
      last = now; S.t += dt / 1000;
      const fr = tauri ? await inv('spectrum_frame', { dtMs: dt }) : null;
      drawSpectrum(fr ? { db: fr.db, step: fr.transient ? 1 : Math.max(0, (S.step || 0) * 0.82) } : null);
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}
boot();
