const $ = (id) => document.getElementById(id);
const el = (tag, text, className) => { const n = document.createElement(tag); if (text !== undefined) n.textContent = text; if (className) n.className = className; return n; };

// The maps servers can rotate through. `slug` names the capture in
// /assets/maps/; servers report "<Map name> - <Mode>".
const MAPS = [
  { slug: 'old-holler', name: 'Old Holler', modes: ['CTF', 'C&H', 'TDM'], text: 'A flooded river ravine between two powered halls. Take the bridge, or wade through the water and lose your speed.' },
  { slug: 'tower-complex', name: 'Tower Complex', modes: ['CTF', 'C&H', 'TDM'], text: 'Floating towers over rolling hills. Jet ledges, open atriums and a long fall if you miss.' },
  { slug: 'cairnhold', name: 'Cairnhold', modes: ['CTF', 'C&H', 'TDM'], text: 'Hillside bunkers joined to their flag towers by covered trenches, with a stone ring on the saddle.' },
  { slug: 'frostline', name: 'Frostline', modes: ['CTF', 'C&H', 'TDM'], text: 'Polar research stations on steep shelves. Whiteout fog, a beacon ridge and an ice cavern.' },
  { slug: 'dustreach', name: 'Dustreach', modes: ['CTF', 'C&H', 'TDM'], text: 'Sandstone citadels in the dunes, a sunken flag court, and sewers under the Sun Gate.' },
  { slug: 'reefbreak', name: 'Reefbreak', modes: ['CTF', 'C&H', 'TDM'], text: 'Two ships hover over a shallow reef. Skim the water at full speed; a lighthouse holds the middle.' },
  { slug: 'ozarktic-blast', name: 'Ozarktic Blast', modes: ['CTF', 'C&H', 'TDM'], text: 'Frosted pine highlands split by a tall central mountain: one team on a giant hill, one in a deep valley.' },
  { slug: 'ashveil', name: 'Ashveil', modes: ['CTF', 'C&H', 'TDM'], text: 'A spired temple over a buried cathedral in a caldera ringed by a lava sea, the bones of giant beasts half sunk in the ash and a skull tower in a lava moat. Touch the post in the cathedral and the spires strike enemy flyers.' },
  { slug: 'longfield', name: 'Longfield', modes: ['Football'], text: 'A floodlit stadium for Football: passes, fumbles, tackles and touchdowns.' },
  { slug: 'highgoal', name: 'Highgoal', modes: ['Football'], text: 'A walled Football arena with the goals up on 7 m platforms. Jet to score.' },
  { slug: 'tenfold', name: 'Tenfold Tower', modes: ['Duel lobby'], text: 'The duel servers\' lobby: ten floors round a spiral stair, from a cargo bay to a crystal cavern and a throne hall. Inside, everyone holds a blade and nothing counts while duels play.' },
];
// Map variants the site has a capture of (a server's own maps otherwise
// show the default image).
const VARIANTS = { 'old holler at dusk': 'old-holler-dusk' };
const slugFor = (mapLabel) => {
  const name = String(mapLabel || '').split(' - ')[0].trim().toLowerCase();
  return MAPS.find(m => m.name.toLowerCase() === name)?.slug || VARIANTS[name];
};

// What a server runs beyond the base game (docs/mods): its mods, and maps
// other than the ones every game ships (the list above). It comes from the server, so it is
// shown as plain text, shortened and capped.
const BASE_MAPS = new Set(MAPS.map(m => m.slug));
function extras(status) {
  const items = Array.isArray(status?.content) ? status.content.slice(0, 32) : [];
  const ok = (i) => i && typeof i.name === 'string' && typeof i.key === 'string' && Number.isFinite(i.bytes) && i.bytes >= 0;
  const mods = items.filter(i => ok(i) && i.kind === 'mod');
  const maps = items.filter(i => ok(i) && (i.kind ?? 'map') === 'map' && !BASE_MAPS.has(i.key));
  return { mods: mods.map(i => i.name.slice(0, 48)), maps: maps.map(i => i.name.slice(0, 48)),
    bytes: [...mods, ...maps].reduce((n, i) => n + i.bytes, 0) };
}
const size = (b) => b >= 1e6 ? `${(b / 1e6).toFixed(1)} MB` : `${Math.max(1, Math.round(b / 1e3))} KB`;
function chips(x, limit) {
  const row = el('span', undefined, 'chips'), all = [...x.mods.map(n => ['mod', n]), ...x.maps.map(n => ['map', n])];
  for (const [kind, name] of all.slice(0, limit)) row.append(el('span', `${kind === 'mod' ? 'MOD' : 'MAP'} ${name}`, `chip ${kind}`));
  if (all.length > limit) row.append(el('span', `+${all.length - limit} more`, 'chip size'));
  if (x.bytes > 0) row.append(el('span', `+${size(x.bytes)}`, 'chip size'));
  return row;
}
const mapImage = (slug, size) => slug ? `/assets/maps/${slug}${size ? `-${size}` : ''}.jpg` : '/assets/maps/tower-complex-640.jpg';

// The live server list: one line per server, sortable and filtered like
// the game's Find match table. A click opens the server's map, score and
// roster under its line, fetched then and on each refresh while it's open
// (the listing alone is one request, however many servers there are).
// `hosts`: the directory's `servers` (CA-certified). `community`: its
// `vouched` servers, which have their own keys, joined by invite link.
let hosts = [], community = [], opened = null, selectedPlayer = null, details = new Map(), busy = false;
let sortBy = { key: 'players', dir: -1 };
const communityOk = (h) => h && typeof h.id === 'string' && typeof h.name === 'string' && typeof h.host === 'string'
  && /^quic:\/\/[^\s/?#@]+$/.test(h.host) && typeof h.sha256 === 'string' && /^[0-9a-f]{64}$/.test(h.sha256)
  && Number.isInteger(h.players) && Number.isInteger(h.max_players);
const inviteLink = (h) => `peakrunner://join/${h.host.slice('quic://'.length)}#sha256=${h.sha256}`;
async function json(path) { const r = await fetch(path, { cache: 'no-store', signal: AbortSignal.timeout(6000) }); if (!r.ok) throw Error(`HTTP ${r.status}`); return r.json(); }
function stat(label, value) { const n = el('div', undefined, 'stat'); n.append(el('span', label), el('strong', String(value))); return n; }

const mapOf = (h) => String(h.map || '').split(' - ')[0].trim();
const modeOf = (h) => String(h.map || '').split(' - ')[1]?.trim() || '';
const people = (h) => h.players - (Number.isInteger(h.bots) ? h.bots : 0);
const COLUMNS = [['name', 'Server'], ['mode', 'Mode'], ['map', 'Map'], ['players', 'Players'], ['location', 'Location']];
const valueOf = (h, key) => key === 'mode' ? modeOf(h) : key === 'map' ? mapOf(h) : key === 'players' ? people(h) * 1000 + h.players : key === 'location' ? (h.location || '') : h.name;
function compare(a, b) {
  const x = valueOf(a, sortBy.key), y = valueOf(b, sortBy.key);
  const order = typeof x === 'number' ? x - y : String(x).localeCompare(String(y), undefined, { sensitivity: 'base' });
  return order * sortBy.dir || a.name.localeCompare(b.name);
}
function shown(h) {
  const text = $('server-filter').value.trim().toLowerCase(), mode = $('mode-filter').value;
  if (mode && modeOf(h) !== mode) return false;
  if ($('hide-empty').checked && people(h) === 0) return false;
  if ($('hide-full').checked && people(h) >= h.max_players) return false;
  return !text || [h.name, h.map, h.location].some(v => String(v || '').toLowerCase().includes(text));
}
// The modes on offer, from what servers report.
function modeOptions() {
  const select = $('mode-filter'), current = select.value;
  const modes = [...new Set([...hosts, ...community].map(modeOf).filter(Boolean))].sort();
  select.replaceChildren(el('option', 'All modes'), ...modes.map(m => el('option', m)));
  select.options[0].value = '';
  select.value = modes.includes(current) ? current : '';
}

// An opened server: its map in full, score, address, mods and roster.
function detailsPanel(host, isCommunity) {
  const root = el('div', undefined, 'server-details'), s = details.get(host.id);
  if (s === undefined) { root.append(el('p', 'Fetching the live details…', 'subtle')); return root; }
  if (!s) { root.append(el('p', 'Live details are unavailable. The server may be restarting; refresh in a moment.', 'empty')); return root; }
  const img = el('img', undefined, 'details-map'); img.src = mapImage(slugFor(s.map), 640); img.alt = `${mapOf(s) || 'The map'}, captured in PeakRunner`; img.width = 640; img.height = 400;
  const side = el('div', undefined, 'details-side');
  const grid = el('div', undefined, 'match-grid'); grid.append(stat('Map', s.map), stat('Players', `${s.players} / ${s.max_players}`), stat('Location', s.location || host.location || 'Not stated'));
  side.append(grid);
  if (modeOf(s) === 'DUEL') {
    side.append(el('p', 'A duel server: everyone meets in the lobby and challenges someone, or a bot, from the duel board. Duels play side by side, one on one.', 'note'));
  } else {
    const scores = el('div', undefined, 'match-grid'); const time = Math.max(0, Math.floor(s.time_left));
    scores.append(stat('Ember', s.score[0]), stat('Glacier', s.score[1]), stat('Time left', `${Math.floor(time / 60)}:${String(time % 60).padStart(2, '0')}`));
    side.append(scores);
  }
  side.append(el('p', `Game version ${s.game_version || 'not reported'}`, 'version'));
  // Community servers join by invite link (it carries the key to check).
  const target = isCommunity ? inviteLink(host) : host.host;
  const address = el('div', undefined, 'address-row'), copy = el('button', isCommunity ? 'Copy invite link' : 'Copy address', 'button small'); copy.type = 'button';
  copy.addEventListener('click', async () => { try { await navigator.clipboard.writeText(target); copy.textContent = 'Copied'; } catch { copy.textContent = `Select the ${isCommunity ? 'link' : 'address'} to copy`; } });
  address.append(el('code', target), copy);
  side.append(address);
  if (isCommunity) side.append(el('p', 'Community server. Paste the invite link into Find match → Direct server address or invite link.', 'note community-note'));
  const top = el('div', undefined, 'details-top'); top.append(img, side); root.append(top);
  const x = extras(s);
  if (x.mods.length || x.maps.length) {
    const box = el('div', undefined, 'mods-box');
    box.append(el('h3', 'Mods and extra maps'), chips(x, 32),
      el('p', `Joining downloads ${size(x.bytes)} from this server, after you agree, and keeps it for next time. Your game only ever receives data.`));
    const more = el('a', 'How mods work →', 'text-link'); more.href = '/docs/mods/';
    box.append(more);
    root.append(box);
  }
  root.append(el('h3', 'Players'));
  const roster = Array.isArray(s.roster) ? s.roster : [];
  if (!roster.length) root.append(el('p', 'Nobody yet. The next drop is yours.', 'empty'));
  else {
    const table = el('table', undefined, 'roster'), head = el('thead'), tr = el('tr');
    for (const title of ['Player', 'Team', 'Frags', 'Deaths']) tr.append(el('th', title)); head.append(tr); table.append(head);
    const body = el('tbody');
    for (const p of roster) { const row = el('tr'), cell = el('td'), button = el('button', p.bot ? `${p.name} · bot` : p.name, 'player-button'); button.type = 'button'; button.addEventListener('click', () => { selectedPlayer = p.id; showPlayer(root, roster); }); cell.append(button); row.append(cell, el('td', p.team), el('td', String(p.frags)), el('td', String(p.deaths))); body.append(row); }
    table.append(body); const detail = el('div', undefined, 'player-detail'); root.append(table, detail); showPlayer(root, roster);
    if (roster.some(p => p.bot)) root.append(el('p', modeOf(s) === 'DUEL' ? 'Bots spar with blades in the tower: each person who joins takes a bot\'s place, and anyone can challenge one from the duel board.' : 'Bots keep the match going: each player who joins takes a bot\'s place.', 'subtle'));
  }
  root.append(el('p', 'Names are chosen by players, not verified accounts. Scores are for the current match only.', 'subtle'));
  return root;
}
function showPlayer(root, roster) { const node = root.querySelector('.player-detail'); if (!node) return; const p = roster.find(p => p.id === selectedPlayer); node.textContent = p ? `${p.name} · ${p.bot ? 'bot · ' : ''}${p.team} · ${p.frags} frags / ${p.deaths} deaths · K/D ${p.deaths ? (p.frags / p.deaths).toFixed(2) : '— (no deaths)'}` : 'Select a player to see their match stats.'; }

async function fetchDetails(id) {
  try { details.set(id, await json(`/api/servers/${encodeURIComponent(id)}`)); } catch { details.set(id, null); }
}
async function toggle(id) {
  if (opened === id) { opened = null; renderHosts(); return; }
  opened = id; selectedPlayer = null; renderHosts();
  if (!details.has(id)) { await fetchDetails(id); if (opened === id) renderHosts(); }
}

function renderHosts() {
  const root = $('server-list');
  const focused = document.activeElement?.dataset?.server;
  const all = [...hosts.map(h => [h, false]), ...community.map(h => [h, true])];
  if (!all.length) { root.replaceChildren(el('p', 'No servers are reporting right now. Check back shortly.', 'empty')); return; }
  const table = el('table', undefined, 'server-table'), head = el('thead'), tr = el('tr');
  tr.append(el('th', undefined, 'dot-col'));
  for (const [key, title] of COLUMNS) {
    const th = el('th', undefined, `col-${key}`), button = el('button', title, 'sort'); button.type = 'button';
    if (sortBy.key === key) { th.setAttribute('aria-sort', sortBy.dir > 0 ? 'ascending' : 'descending'); button.append(el('span', sortBy.dir > 0 ? ' ▲' : ' ▼', 'arrow')); }
    button.addEventListener('click', () => { sortBy = sortBy.key === key ? { key, dir: -sortBy.dir } : { key, dir: key === 'players' ? -1 : 1 }; renderHosts(); });
    th.append(button); tr.append(th);
  }
  tr.append(el('th', undefined, 'open-col'));
  head.append(tr); table.append(head);
  const body = el('tbody'), rows = all.filter(([h]) => shown(h)).sort(([a], [b]) => compare(a, b));
  for (const [h, isCommunity] of rows) {
    const open = opened === h.id, row = el('tr', undefined, `server-row${open ? ' open' : ''}${people(h) ? ' busy' : ''}`);
    row.addEventListener('click', (e) => { if (!e.target.closest('button')) toggle(h.id); });
    const name = el('td', undefined, 'col-name'), button = el('button', h.name, 'server-name'); button.type = 'button';
    button.dataset.server = h.id; button.setAttribute('aria-expanded', String(open)); button.setAttribute('aria-controls', `details-${h.id}`);
    button.addEventListener('click', () => toggle(h.id));
    name.append(button);
    if (isCommunity) name.append(el('span', 'community', 'tag small-tag'));
    const count = el('td', undefined, 'col-players');
    count.append(el('strong', `${h.players}/${h.max_players}`));
    if (h.bots) count.append(el('small', ` · ${h.bots} bot${h.bots === 1 ? '' : 's'}`));
    const dot = el('td', undefined, 'dot-col'); dot.append(el('i', undefined, 'live-dot'));
    row.append(dot, name, el('td', modeOf(h) || '—', 'col-mode'), el('td', mapOf(h) || 'Not reported', 'col-map'), count,
      el('td', h.location || '—', 'col-location'), el('td', open ? '▾' : '▸', 'open-col'));
    body.append(row);
    if (open) {
      const more = el('tr', undefined, 'details-row'), cell = el('td'); cell.colSpan = COLUMNS.length + 2; cell.id = `details-${h.id}`;
      cell.append(detailsPanel(h, isCommunity)); more.append(cell); body.append(more);
    }
  }
  if (!rows.length) { const none = el('tr'), cell = el('td', 'No servers match these filters.', 'empty-row'); cell.colSpan = COLUMNS.length + 2; none.append(cell); body.append(none); }
  table.append(body); root.replaceChildren(table);
  if (focused) root.querySelector(`[data-server="${CSS.escape(focused)}"]`)?.focus();
}
async function refresh() {
  if (busy) return; busy = true; $('refresh').disabled = true;
  try {
    const data = await json('/api/servers'); if (!Array.isArray(data.servers)) throw Error('Invalid directory'); hosts = data.servers;
    community = Array.isArray(data.vouched) ? data.vouched.filter(communityOk).slice(0, 512) : [];
    // Only the opened server's details, fresh with the listing.
    if (opened && [...hosts, ...community].some(h => h.id === opened)) await fetchDetails(opened);
    else opened = null;
    modeOptions(); renderHosts();
    const count = [...hosts, ...community].reduce((n, h) => n + h.players, 0);
    const extra = community.length ? ` · ${community.length} community` : '';
    $('network-status').textContent = `${hosts.length} server${hosts.length === 1 ? '' : 's'} online${extra} · ${count} player${count === 1 ? '' : 's'}`;
    $('updated').textContent = `Updated ${new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })}`;
  } catch {
    hosts = []; community = []; details.clear(); opened = null;
    $('server-list').replaceChildren(el('p', 'The server directory is unreachable right now. Downloads still work; try Refresh in a moment.', 'empty'));
    $('network-status').textContent = 'Directory unavailable'; $('updated').textContent = 'Live status unavailable';
  } finally { busy = false; $('refresh').disabled = false; }
}
for (const id of ['server-filter', 'mode-filter', 'hide-empty', 'hide-full']) $(id).addEventListener('input', renderHosts);

// How many people have played on the official servers, in all and today
// (UTC): the match servers write the counts (/stats/stats.json). Hidden
// when they can't be read.
async function played() {
  const line = $('played'); if (!line) return;
  try {
    const s = await json('/stats/stats.json');
    if (!Number.isInteger(s.total) || !Number.isInteger(s.today) || typeof s.day !== 'string' || s.total < 1) throw Error('No counts');
    // Counts from an earlier day (no one has joined since midnight) say none today.
    const today = s.day === new Date().toISOString().slice(0, 10) ? s.today : 0;
    line.replaceChildren(el('strong', s.total.toLocaleString()), ` ${s.total === 1 ? 'person has' : 'people have'} played on the official servers · `,
      el('strong', today.toLocaleString()), ' today (UTC)');
    line.hidden = false;
  } catch { line.hidden = true; }
}

function renderMaps() {
  const root = $('map-grid'); if (!root) return;
  for (const m of MAPS) {
    const card = el('article', undefined, 'map-card');
    const img = el('img'); img.src = mapImage(m.slug, 640); img.alt = `${m.name}, captured in PeakRunner`; img.width = 640; img.height = 400; img.loading = 'lazy';
    const body = el('div', undefined, 'map-body'); const tags = el('div', undefined, 'tags');
    for (const mode of m.modes) tags.append(el('span', mode, 'tag'));
    body.append(el('h3', m.name), el('p', m.text), tags); card.append(img, body); root.append(card);
  }
}

// Downloads: launchers first (the visitor's OS highlighted), standalone
// packages in the fold-out below.
function visitorOs() {
  const p = (navigator.userAgentData?.platform || navigator.platform || navigator.userAgent || '').toLowerCase();
  if (p.includes('mac')) return 'macos'; if (p.includes('win')) return 'windows'; if (p.includes('linux') && !p.includes('android')) return 'linux'; return null;
}
const osOf = (d) => d.os || (/mac/i.test(d.platform) ? 'macos' : /win/i.test(d.platform) ? 'windows' : /linux/i.test(d.platform) ? 'linux' : null);
const kindOf = (d) => d.kind || (/launcher/i.test(d.title) ? 'launcher' : 'standalone');
function downloadCard(d, mine) {
  const card = el('article', undefined, `download-card${mine ? ' mine' : ''}`);
  if (mine) card.append(el('p', 'Recommended for this computer', 'badge'));
  card.append(el('p', d.platform, 'eyebrow'), el('h3', d.title), el('p', d.description));
  if (d.url && d.url.startsWith('/downloads/')) {
    const a = el('a', d.label, `button${mine ? ' primary' : ''}`); a.href = d.url; a.setAttribute('download', ''); card.append(a);
    if (d.sha256) { const sum = el('details', undefined, 'checksum'); sum.append(el('summary', 'SHA-256'), el('code', d.sha256)); card.append(sum); }
  } else card.append(el('p', 'Not published yet', 'subtle'));
  return card;
}
async function downloads() {
  try {
    const release = await json('/release.json'); $('release-version').textContent = `Game ${release.version} · launcher ${release.launcher_version}`;
    const os = visitorOs(), launchers = $('launcher-list'), standalone = $('standalone-list');
    launchers.replaceChildren(); standalone.replaceChildren();
    const order = (list) => list.slice().sort((a, b) => (osOf(b) === os) - (osOf(a) === os));
    for (const d of order(release.downloads.filter(d => kindOf(d) === 'launcher'))) launchers.append(downloadCard(d, os && osOf(d) === os));
    for (const d of order(release.downloads.filter(d => kindOf(d) !== 'launcher'))) standalone.append(downloadCard(d, false));
  } catch {
    $('release-version').textContent = 'Release info unavailable';
    $('launcher-list').replaceChildren(el('p', 'Downloads could not be loaded. Please refresh the page.', 'empty'));
  }
}

$('refresh').addEventListener('click', refresh);
// The montage: a poster until it's asked for, then YouTube's
// privacy-enhanced player, so nothing loads from YouTube before a click.
function montage() {
  const box = $('montage'); if (!box) return;
  box.querySelector('.video-poster').addEventListener('click', () => {
    const f = document.createElement('iframe');
    f.src = `https://www.youtube-nocookie.com/embed/${encodeURIComponent(box.dataset.video)}?autoplay=1&rel=0`;
    f.title = box.dataset.title; f.allow = 'autoplay; encrypted-media; picture-in-picture; fullscreen'; f.allowFullscreen = true;
    f.referrerPolicy = 'strict-origin-when-cross-origin';
    box.replaceChildren(f); f.focus();
  }, { once: true });
}
montage(); renderMaps(); refresh(); downloads(); played(); setInterval(() => { if (!document.hidden) refresh(); }, 5000);
setInterval(() => { if (!document.hidden) played(); }, 60000);
