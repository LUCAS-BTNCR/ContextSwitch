// ContextSwitch Companion: lógica (sin conexión). La usa background.js en Zen y los
// tests en Node (tests/extension/companion.test.cjs) con un `browser` simulado.
//
// Permisos: nativeMessaging + contextualIdentities (+ cookies, requerido por Firefox para
// usar cookieStoreId en tabs.create) + tabs, SOLO para comparar el sitio de los Essentials
// con la URL a abrir y verificar que una pestaña sea about:blank antes de cerrarla;
// sessions (reabrir Essentials cerrados) y storage (recordar ventanas de un modo).
// SIN "history" ni "<all_urls>". Nada se envía fuera de la PC.
//
// Cómo cambia de workspace (API pública): Zen no expone sus workspaces a WebExtensions,
// pero tiene la opción "Switch to workspace where container is set as default when
// opening container tabs". Si cada workspace tiene un contenedor por defecto, abrir o
// activar una pestaña de ese contenedor hace que Zen salte a ese workspace.

/* exported ContextSwitchCompanion */
const ContextSwitchCompanion = (() => {
  // Sin distinguir mayúsculas, tildes ni espacios: "entretenimiento " == "Entretenimiento".
  const norm = (s) => (s ?? "").normalize("NFD").replace(/\p{M}/gu, "").trim().toLowerCase();

  // Mismo sitio = mismo host sin "www." (music.youtube.com ≠ youtube.com). Un Essential de
  // YouTube que quedó en un video sigue contando como YouTube.
  const hostOf = (u) => { try { return new URL(u).hostname.replace(/^www\./, ""); } catch { return ""; } };
  const sameSite = (a, b) => { const ha = hostOf(a); return ha !== "" && ha === hostOf(b); };

  // URL web completa (descarta "", "https://" y similares que deja un campo sin completar).
  const isWebUrl = (u) => { try { const x = new URL(u); return /^https?:$/.test(x.protocol) && !!x.hostname; } catch { return false; } };

  const lastUsed = (list) => list.reduce((a, b) => ((a.lastAccessed ?? 0) >= (b.lastAccessed ?? 0) ? a : b));

  /**
   * @param {object} deps
   * @param {any} deps.browser  API WebExtension (real o simulada)
   * @param {(event: string, data?: object) => void} [deps.log]
   * @param {(ms: number) => Promise<void>} [deps.sleep]
   */
  function create({ browser, log = () => {}, sleep = (ms) => new Promise((r) => setTimeout(r, ms)) }) {
    // Ventanas abiertas por openWindows (de un modo): "activate" de otro modo no las usa,
    // así no les cambia el workspace ni las vuelve a mostrar si ContextSwitch las ocultó.
    // Se guarda en storage.local (sobrevive a recargas) y se depura con las ventanas reales.
    const managedWindows = new Set();
    const saveManaged = () => browser.storage.local.set({ managedWindows: [...managedWindows] }).catch(() => {});

    async function init() {
      const { managedWindows: saved = [] } = await browser.storage.local.get("managedWindows").catch(() => ({}));
      const open = new Set((await browser.windows.getAll()).map((w) => w.id));
      for (const id of saved) if (open.has(id)) managedWindows.add(id);
      await saveManaged();
      browser.windows.onRemoved.addListener((id) => { if (managedWindows.delete(id)) saveManaged(); });
      log("managed-restored", { count: managedWindows.size });
    }

    async function containerId(name) {
      if (!name) return undefined;
      const all = await browser.contextualIdentities.query({});
      let hit = all.find((c) => norm(c.name) === norm(name));
      let created = false;
      if (!hit) {
        // No existe: se crea con ese nombre. Falta asignarlo como contenedor por defecto del
        // workspace en Zen (eso Zen no lo expone a extensiones; se hace una vez a mano).
        try {
          hit = await browser.contextualIdentities.create({ name: name.trim(), color: "purple", icon: "chill" });
          created = true;
        } catch (e) {
          log("container-create-error", { wanted: name, error: String(e) });
        }
      }
      log("container", { wanted: name, found: hit?.name ?? null, created, cookieStoreId: hit?.cookieStoreId ?? null,
                         available: all.map((c) => c.name) });
      return hit?.cookieStoreId;
    }

    // Reabre (como Ctrl+Shift+T) la pestaña cerrada más reciente de cada sitio pedido.
    // Usa el SessionStore de Zen, así un Essential vuelve como Essential.
    async function restoreClosed({ hosts = [] }) {
      const want = new Set(hosts.map((h) => h.replace(/^www\./, "")));
      const items = await browser.sessions.getRecentlyClosed({ maxResults: 25 });
      log("closed-list", { sites: items.map((i) => (i.tab ? hostOf(i.tab.url) : `ventana(${i.window?.tabs?.length ?? "?"})`)) });
      const restored = [];
      for (const it of items) {
        const site = it.tab ? hostOf(it.tab.url) : "";
        if (!it.tab || !want.has(site)) continue; // nunca se restauran ventanas enteras
        want.delete(site);
        try {
          const r = await browser.sessions.restore(it.tab.sessionId);
          restored.push(site);
          log("restored", { site, pinned: r?.tab?.pinned ?? null });
        } catch (e) {
          log("restore-error", { site, error: String(e) });
        }
      }
      if (want.size) log("not-found", { sites: [...want] });
      return restored;
    }

    // ContextSwitch reusó ventanas suyas: se marcan como "de un modo" las ventanas cuya pestaña
    // activa es de ese contenedor y de uno de esos sitios (por si se abrieron antes de recordarlas).
    async function adoptWindows({ container, urls = [] }) {
      const cookieStoreId = await containerId(container);
      const active = await browser.tabs.query({ active: true });
      let n = 0;
      for (const t of active) {
        if ((!cookieStoreId || t.cookieStoreId === cookieStoreId) && urls.some((u) => sameSite(t.url, u)) && !managedWindows.has(t.windowId)) {
          managedWindows.add(t.windowId); n++;
        }
      }
      if (n) await saveManaged();
      log("adopted", { count: n, total: managedWindows.size });
      return n;
    }

    // Una ventana por URL, en orden. ContextSwitch identifica cada ventana por el orden en
    // que aparece (no lee títulos ni URLs), así que se crean de a una con una pausa.
    async function openWindows({ container, urls = [] }) {
      const cookieStoreId = await containerId(container);
      const results = [];
      for (const url of urls.filter(isWebUrl)) {
        try {
          // Zen ignora cookieStoreId en windows.create: la pestaña inicial toma el contenedor
          // del workspace activo. En cambio, una PESTAÑA de contenedor abierta en la ventana
          // sí respeta el contenedor y hace que Zen cambie esa ventana a su workspace.
          //
          // OJO: una ventana nueva de Zen trae los Essentials (pestañas fijadas). Nunca se
          // cierra nada que no hayamos creado nosotros: solo la about:blank de windows.create
          // (verificada: no fijada y todavía en about:blank) o nuestra pestaña si se usa un Essential.
          const w = await browser.windows.create({ url: "about:blank", focused: true });
          managedWindows.add(w.id);
          await saveManaged();
          const ourBlank = (w.tabs || []).filter((t) => !t.pinned && t.url === "about:blank").map((t) => t.id);
          const t = await browser.tabs.create({ windowId: w.id, url, cookieStoreId, active: true });
          await sleep(400); // Zen pasa la ventana al workspace del contenedor

          // ¿Hay un Essential del mismo sitio en este workspace? Entonces se usa ese.
          let shown = t;
          const pinned = await browser.tabs.query({ windowId: w.id, pinned: true });
          const essential = pinned.find((p) => sameSite(p.url, url) && (!cookieStoreId || p.cookieStoreId === cookieStoreId));
          if (essential) {
            await browser.tabs.update(essential.id, { active: true });
            await browser.tabs.remove(t.id); // la que creamos recién
            shown = essential;
          }
          for (const id of ourBlank) {
            if (id === shown.id) continue;
            const b = await browser.tabs.get(id).catch(() => null);
            if (b && !b.pinned && b.url === "about:blank") await browser.tabs.remove(id);
          }
          results.push({ windowId: w.id, usedEssential: !!essential });
          log("window-created", { windowId: w.id, requested: cookieStoreId ?? null, tabCookieStoreId: shown.cookieStoreId ?? null,
                                  usedEssential: !!essential, pinnedInWindow: pinned.length });
        } catch (e) {
          log("window-error", { error: String(e) });
        }
        await sleep(500);
      }
      return results;
    }

    async function activate({ container, urls = [], unloadInactive: unload }) {
      const cookieStoreId = await containerId(container);
      const wanted = urls.filter(isWebUrl);

      // Pestañas de ese contenedor (= de ese workspace), fuera de las ventanas de otros modos.
      const existing = (cookieStoreId ? await browser.tabs.query({ cookieStoreId }) : [])
        .filter((t) => !managedWindows.has(t.windowId));

      let focus = null;
      const reused = [], created = [];
      for (const url of wanted) {
        // Reciclar antes que abrir: 1) un Essential/fijada de ese sitio, 2) cualquier pestaña de ese sitio.
        const same = existing.filter((t) => sameSite(t.url, url));
        const hit = same.find((t) => t.pinned) ?? (same.length ? lastUsed(same) : null);
        if (hit) { reused.push(hostOf(url)); focus ??= hit; continue; }
        // tabs.create rechaza URLs privilegiadas como about:newtab.
        const t = await browser.tabs.create({ url, cookieStoreId, active: !focus });
        created.push(hostOf(url));
        focus ??= t;
      }
      if (!wanted.length) {
        // Sin URLs: ir al workspace activando la última pestaña usada de ese contenedor.
        focus = existing.length ? lastUsed(existing) : await browser.tabs.create({ cookieStoreId, active: true });
      }
      if (focus) {
        await browser.tabs.update(focus.id, { active: true }); // pestaña de ese contenedor → Zen pasa a su workspace
        await browser.windows.update(focus.windowId, { focused: true });
      }
      log("activated", { container, reused, created });

      if (unload) await unloadInactive();
      return { reused, created };
    }

    async function unloadInactive() {
      const tabs = await browser.tabs.query({ active: false, discarded: false, audible: false });
      // Nunca las fijadas (Essentials). De a una: si una se cerró mientras tanto, no frena al resto.
      for (const t of tabs.filter((t) => !t.pinned)) {
        await browser.tabs.discard(t.id).catch(() => {});
      }
    }

    async function handle(msg) {
      switch (msg?.type) {
        case "activate": return activate(msg);
        case "unloadInactive": return unloadInactive();
        case "openWindows": return openWindows(msg);
        case "restoreClosed": return restoreClosed(msg);
        case "adoptWindows": return adoptWindows(msg);
        default: return undefined; // desconocido: se ignora
      }
    }

    return { init, handle, containerId, activate, openWindows, restoreClosed, adoptWindows, unloadInactive, managedWindows };
  }

  return { create, norm, sameSite, hostOf, isWebUrl };
})();

if (typeof module !== "undefined") module.exports = ContextSwitchCompanion;
