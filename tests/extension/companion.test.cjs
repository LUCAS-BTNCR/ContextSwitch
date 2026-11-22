// Tests de extension-zen/lib.js con el runner nativo de Node (sin dependencias).
//   node --test tests/extension/
const test = require("node:test");
const assert = require("node:assert/strict");
// lib.js se carga como lo hace Zen: script clásico (no módulo). El package.json del
// proyecto es "type": "module", así que require() lo trataría como ESM.
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
// CS_LIB permite correr la suite contra una copia modificada (prueba de mutación).
const libPath = process.env.CS_LIB || path.join(__dirname, "../../extension-zen/lib.js");
const sandbox = { module: { exports: {} }, URL, setTimeout, console };
vm.runInNewContext(fs.readFileSync(libPath, "utf8"), sandbox, { filename: libPath });
const CS = sandbox.module.exports;
// Los objetos creados dentro de la sandbox tienen otros prototipos: se normalizan para comparar.
const plain = (x) => JSON.parse(JSON.stringify(x));
const { FakeZen } = require("./fake-zen.cjs");

const noSleep = async () => {};
function setup(opts) {
  const zen = new FakeZen(opts);
  const logs = [];
  const c = CS.create({ browser: zen.browser, log: (event, data) => logs.push({ event, ...data }), sleep: noSleep });
  return { zen, c, logs };
}
const ESSENTIALS = [
  { url: "https://www.youtube.com/watch?v=abc", container: "Personal" },
  { url: "https://music.youtube.com/", container: "Personal" },
  { url: "https://github.com/", container: "Work" },
  { url: "https://web.whatsapp.com/", container: "Personal" },
];

// ── helpers puros ────────────────────────────────────────────────────────────
test("norm: ignora mayúsculas, tildes y espacios", () => {
  assert.equal(CS.norm(" Entretenimiento "), "entretenimiento");
  assert.equal(CS.norm("Música"), CS.norm("musica"));
  assert.equal(CS.norm(null), "");
});

test("sameSite: mismo host sin www, subdominios distintos son otro sitio", () => {
  assert.ok(CS.sameSite("https://www.youtube.com/watch?v=1", "https://youtube.com/"));
  assert.ok(!CS.sameSite("https://music.youtube.com/", "https://www.youtube.com/"));
  assert.ok(!CS.sameSite("no-url", "no-url"), "dos URLs inválidas no son 'el mismo sitio'");
  assert.ok(!CS.sameSite(undefined, "https://a.com"));
});

test("isWebUrl: descarta campos incompletos y esquemas no web", () => {
  assert.ok(CS.isWebUrl("https://github.com/"));
  assert.ok(CS.isWebUrl("http://localhost:3000"));
  for (const bad of ["", "https://", "about:blank", "file:///C:/x", "javascript:alert(1)", undefined]) {
    assert.ok(!CS.isWebUrl(bad), String(bad));
  }
});

// ── contenedores ─────────────────────────────────────────────────────────────
test("containerId encuentra el contenedor aunque difieran mayúsculas/tildes", async () => {
  const { zen, c } = setup({ containers: ["Personal", "Música"] });
  assert.equal(await c.containerId("musica "), zen.csid("Música"));
});

test("containerId crea el contenedor si no existe, y sin nombre no hace nada", async () => {
  const { zen, c } = setup();
  const id = await c.containerId("Entretenimiento");
  assert.equal(id, zen.csid("Entretenimiento"));
  assert.equal(await c.containerId(""), undefined);
  assert.equal(await c.containerId(undefined), undefined);
});

test("containerId: si no se puede crear, devuelve undefined y lo registra", async () => {
  const { zen, c, logs } = setup();
  zen.failCreateContainer = true;
  assert.equal(await c.containerId("Nuevo"), undefined);
  assert.ok(logs.some((l) => l.event === "container-create-error"));
});

// ── openWindows: el invariante crítico ──────────────────────────────────────
test("CRÍTICO: openWindows nunca cierra Essentials ni pestañas que no creó", async () => {
  const { zen, c } = setup({ essentials: ESSENTIALS });
  const before = new Set(zen.tabs.keys());
  await c.openWindows({ container: "Personal", urls: ["https://www.youtube.com/", "https://example.org/", "https://github.com/"] });

  for (const id of zen.removed) {
    assert.ok(!before.has(id), `cerró la pestaña ${id} que existía antes`);
  }
  const pinnedAlive = [...zen.tabs.values()].filter((t) => t.pinned).length;
  assert.equal(pinnedAlive, ESSENTIALS.length * 3, "cada ventana conserva TODAS sus copias de Essentials");
  assert.ok(zen.removed.every((id) => !zen.pinnedIds().includes(id)));
});

test("openWindows usa el Essential del mismo sitio y contenedor en vez de abrir otra pestaña", async () => {
  const { zen, c } = setup({ essentials: ESSENTIALS });
  const [r] = await c.openWindows({ container: "Personal", urls: ["https://www.youtube.com/"] });
  assert.equal(r.usedEssential, true);
  const tabsInWindow = [...zen.tabs.values()].filter((t) => t.windowId === r.windowId);
  assert.ok(tabsInWindow.every((t) => t.pinned), "quedan solo los Essentials: ni la blank ni la duplicada");
  const yt = tabsInWindow.find((t) => t.url.includes("youtube.com/watch"));
  assert.equal(yt.active, true, "el Essential de YouTube queda activo");
});

test("openWindows NO usa un Essential del mismo sitio si es de otro contenedor", async () => {
  const { zen, c } = setup({ essentials: [{ url: "https://github.com/", container: "Work" }] });
  const [r] = await c.openWindows({ container: "Personal", urls: ["https://github.com/"] });
  assert.equal(r.usedEssential, false);
  const own = [...zen.tabs.values()].find((t) => t.windowId === r.windowId && !t.pinned);
  assert.equal(own.cookieStoreId, zen.csid("Personal"), "abre su propia pestaña en el contenedor pedido");
});

test("openWindows sin Essential: deja su pestaña y cierra solo su about:blank", async () => {
  const { zen, c } = setup({ essentials: ESSENTIALS });
  const [r] = await c.openWindows({ container: "Personal", urls: ["https://example.org/"] });
  const mine = [...zen.tabs.values()].filter((t) => t.windowId === r.windowId && !t.pinned);
  assert.deepEqual(mine.map((t) => t.url), ["https://example.org/"]);
  assert.equal(zen.removed.length, 1, "solo la about:blank inicial");
});

test("openWindows no cierra la pestaña inicial si ya no es about:blank", async () => {
  const { zen, c } = setup();
  const realCreate = zen.browser.windows.create;
  zen.browser.windows.create = async (o) => {
    const w = await realCreate(o);
    zen.tabs.get(w.tabs[0].id).url = "https://el-usuario-navego.com/"; // cambió entre medio
    return w;
  };
  await c.openWindows({ container: "Personal", urls: ["https://example.org/"] });
  assert.equal(zen.removed.length, 0);
});

test("openWindows ignora URLs incompletas sin crear ventanas", async () => {
  const { zen, c } = setup();
  const r = await c.openWindows({ container: "Work", urls: ["https://", "", "https://github.com/"] });
  assert.equal(r.length, 1);
  assert.equal(zen.windows.size, 1);
});

test("openWindows recuerda sus ventanas (también en storage) para que activate no las use", async () => {
  const { zen, c } = setup();
  const [r] = await c.openWindows({ container: "Personal", urls: ["https://example.org/"] });
  assert.ok(c.managedWindows.has(r.windowId));
  assert.deepEqual(plain(zen.storageData.managedWindows), [r.windowId]);
});

test("openWindows: si falla una ventana, sigue con las demás y lo registra", async () => {
  const { zen, c, logs } = setup();
  zen.failTabsCreateFor = "https://falla.com/";
  const r = await c.openWindows({ container: "Personal", urls: ["https://falla.com/", "https://ok.com/"] });
  assert.equal(r.length, 1);
  assert.ok(logs.some((l) => l.event === "window-error"));
});

// ── activate (modo pestañas) ────────────────────────────────────────────────
test("activate prefiere el Essential sobre una pestaña común del mismo sitio", async () => {
  const { zen, c } = setup();
  const w = zen.addWindow();
  zen.addTab({ windowId: w, url: "https://github.com/a", cookieStoreId: zen.csid("Work"), lastAccessed: 999 });
  const ess = zen.addTab({ windowId: w, url: "https://github.com/", cookieStoreId: zen.csid("Work"), pinned: true, lastAccessed: 1 });
  const r = await c.activate({ container: "Work", urls: ["https://github.com/"] });
  assert.deepEqual(plain(r), { reused: ["github.com"], created: [] });
  assert.equal(zen.tabs.get(ess.id).active, true);
  assert.equal(zen.created.length, 0, "no abre pestañas nuevas");
});

test("activate recicla la pestaña común más usada si no hay Essential", async () => {
  const { zen, c } = setup();
  const w = zen.addWindow();
  zen.addTab({ windowId: w, url: "https://github.com/x", cookieStoreId: zen.csid("Work"), lastAccessed: 5 });
  const newest = zen.addTab({ windowId: w, url: "https://github.com/y", cookieStoreId: zen.csid("Work"), lastAccessed: 50 });
  await c.activate({ container: "Work", urls: ["https://github.com/"] });
  assert.equal(zen.tabs.get(newest.id).active, true);
  assert.equal(zen.created.length, 0);
});

test("activate abre pestaña solo para lo que no existe, y enfoca la primera URL", async () => {
  const { zen, c } = setup();
  zen.addWindow();
  const r = await c.activate({ container: "Work", urls: ["https://a.com/", "https://b.com/"] });
  assert.deepEqual(plain(r.created), ["a.com", "b.com"]);
  const [a, b] = zen.created.map((id) => zen.tabs.get(id));
  assert.equal(a.active, true);
  assert.equal(b.active, false);
  assert.equal(a.cookieStoreId, zen.csid("Work"));
});

test("activate ignora pestañas que viven en ventanas de otros modos", async () => {
  const { zen, c } = setup();
  zen.addWindow(); // la ventana normal del usuario (ahí abre Zen las pestañas nuevas)
  const [mine] = await c.openWindows({ container: "Work", urls: ["https://github.com/"] });
  const before = zen.created.length;
  await c.activate({ container: "Work", urls: ["https://github.com/"] });
  assert.equal(zen.created.length, before + 1, "no reusa la pestaña de la ventana del otro modo");
  const nueva = zen.tabs.get(zen.created.at(-1));
  assert.notEqual(nueva.windowId, mine.windowId);
});

test("activate sin URLs va a la última pestaña usada del contenedor, o abre una si no hay", async () => {
  const { zen, c } = setup();
  const w = zen.addWindow();
  const t = zen.addTab({ windowId: w, url: "https://x.com/", cookieStoreId: zen.csid("Work"), lastAccessed: 9 });
  await c.activate({ container: "Work", urls: [] });
  assert.equal(zen.tabs.get(t.id).active, true);
  assert.equal(zen.created.length, 0);

  const { zen: z2, c: c2 } = setup();
  z2.addWindow();
  await c2.activate({ container: "Personal", urls: [] });
  assert.equal(z2.created.length, 1);
});

test("unloadInactive nunca descarga Essentials y no se frena si una pestaña desaparece", async () => {
  const { zen, c } = setup();
  const w = zen.addWindow();
  const ess = zen.addTab({ windowId: w, url: "https://a.com/", pinned: true });
  const t1 = zen.addTab({ windowId: w, url: "https://b.com/" });
  const t2 = zen.addTab({ windowId: w, url: "https://c.com/" });
  const realDiscard = zen.browser.tabs.discard;
  zen.browser.tabs.discard = async (id) => { if (id === t1.id) throw new Error("Invalid tab ID"); return realDiscard(id); };
  await c.unloadInactive();
  assert.ok(!zen.discarded.includes(ess.id));
  assert.ok(zen.discarded.includes(t2.id), "sigue con la siguiente aunque una falle");
});

// ── restoreClosed / adoptWindows / init ─────────────────────────────────────
test("restoreClosed reabre la más reciente de cada sitio y nunca una ventana entera", async () => {
  const { zen, c } = setup();
  zen.closed = [
    { window: { tabs: [{}, {}, {}] } },
    { tab: { sessionId: "s1", url: "https://github.com/a", pinned: true } },
    { tab: { sessionId: "s2", url: "https://github.com/b", pinned: true } },
    { tab: { sessionId: "s3", url: "https://www.youtube.com/", pinned: true } },
  ];
  const r = await c.restoreClosed({ hosts: ["github.com", "www.youtube.com", "no-esta.com"] });
  assert.deepEqual(zen.restoredIds, ["s1", "s3"]);
  assert.deepEqual(plain(r), ["github.com", "youtube.com"]);
});

test("restoreClosed sigue si una restauración falla", async () => {
  const { zen, c } = setup();
  zen.closed = [
    { tab: { sessionId: "s1", url: "https://a.com/" }, failRestore: true },
    { tab: { sessionId: "s2", url: "https://b.com/" } },
  ];
  await c.restoreClosed({ hosts: ["a.com", "b.com"] });
  assert.deepEqual(zen.restoredIds, ["s2"]);
});

test("adoptWindows marca solo ventanas con pestaña activa de ese contenedor y sitio", async () => {
  const { zen, c } = setup();
  const w1 = zen.addWindow(), w2 = zen.addWindow(), w3 = zen.addWindow();
  zen.addTab({ windowId: w1, url: "https://www.youtube.com/", cookieStoreId: zen.csid("Personal"), active: true });
  zen.addTab({ windowId: w2, url: "https://www.youtube.com/", cookieStoreId: zen.csid("Work"), active: true });
  zen.addTab({ windowId: w3, url: "https://otro.com/", cookieStoreId: zen.csid("Personal"), active: true });
  assert.equal(await c.adoptWindows({ container: "Personal", urls: ["https://www.youtube.com/"] }), 1);
  assert.deepEqual([...c.managedWindows], [w1]);
});

test("init recupera ventanas de un modo que siguen abiertas y olvida las cerradas", async () => {
  const { zen, c } = setup();
  const vive = zen.addWindow();
  zen.storageData.managedWindows = [vive, 999];
  await c.init();
  assert.deepEqual([...c.managedWindows], [vive]);
  zen.removedListeners.forEach((fn) => fn(vive));
  assert.equal(c.managedWindows.size, 0, "al cerrarse una ventana deja de estar marcada");
});

// ── contrato con ContextSwitch (mismos JSON que src-tauri/src/actions/bridge.rs) ──
test("handle despacha los mensajes del protocolo y ignora los desconocidos", async () => {
  const { zen, c } = setup();
  zen.addWindow();
  const msgs = [
    '{"type":"activate","workspace":"Work","container":"Work","urls":["https://github.com/"],"unloadInactive":true}',
    '{"type":"openWindows","container":"Personal","urls":["https://www.youtube.com/"]}',
    '{"type":"adoptWindows","container":null,"urls":["https://www.youtube.com/"]}',
    '{"type":"unloadInactive"}',
  ];
  for (const m of msgs) await c.handle(JSON.parse(m));
  assert.ok(zen.created.length >= 2, "activate y openWindows hicieron su trabajo");
  assert.equal(await c.handle({ type: "algo-nuevo" }), undefined);
  assert.equal(await c.handle(null), undefined);
});
