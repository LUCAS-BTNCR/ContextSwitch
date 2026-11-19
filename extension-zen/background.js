// ContextSwitch Companion para Zen Browser.
//
// Permisos mínimos: nativeMessaging + contextualIdentities (+ cookies, requerido por
// Firefox para usar cookieStoreId en tabs.create). SIN "tabs", "history", "<all_urls>":
// la extensión no puede leer URLs ni títulos de tus pestañas.
//
// Cómo cambia de workspace (API pública): Zen no expone sus workspaces a WebExtensions,
// pero tiene la opción "Switch to workspace where container is set as default when
// opening container tabs". Si cada workspace tiene un contenedor por defecto, abrir o
// activar una pestaña de ese contenedor hace que Zen salte a ese workspace.

const HOST = "contextswitch_bridge";
let port = null;
let retryMs = 1000;

function connect() {
  port = browser.runtime.connectNative(HOST);
  port.onMessage.addListener((msg) => {
    retryMs = 1000; // el host respondió: conexión sana, se resetea el backoff
    handle(msg).catch((e) => console.warn("[cs]", e));
  });
  port.onDisconnect.addListener((p) => {
    if (p.error) console.warn("[cs] host desconectado:", p.error.message);
    port = null;
    setTimeout(connect, retryMs);
    retryMs = Math.min(retryMs * 2, 60_000);
  });
}

async function containerId(name) {
  if (!name) return undefined;
  const found = await browser.contextualIdentities.query({ name });
  return found[0]?.cookieStoreId;
}

async function handle(msg) {
  if (msg.type === "activate") return activate(msg);
  if (msg.type === "unloadInactive") return unloadInactive();
}

async function activate({ container, urls = [], unloadInactive: unload }) {
  const cookieStoreId = await containerId(container);

  // Pestañas del contenedor (sin permiso "tabs" solo obtenemos ids/estado, no URLs).
  const existing = cookieStoreId ? await browser.tabs.query({ cookieStoreId }) : [];

  if (existing.length > 0) {
    // Ya hay pestañas de ese workspace: activar la última usada → Zen cambia de workspace.
    const last = existing.reduce((a, b) => ((a.lastAccessed ?? 0) > (b.lastAccessed ?? 0) ? a : b));
    await browser.tabs.update(last.id, { active: true });
    await browser.windows.update(last.windowId, { focused: true });
  } else {
    // Primera vez: abrir las URLs del perfil en ese contenedor.
    // tabs.create rechaza URLs privilegiadas como about:newtab; sin url abre la nueva pestaña.
    if (urls.length === 0) {
      await browser.tabs.create({ cookieStoreId, active: true });
    }
    for (let i = 0; i < urls.length; i++) {
      await browser.tabs.create({ url: urls[i], cookieStoreId, active: i === 0 });
    }
  }

  if (unload) await unloadInactive();
}

async function unloadInactive() {
  const tabs = await browser.tabs.query({ active: false, discarded: false, audible: false });
  const ids = tabs.filter((t) => !t.pinned).map((t) => t.id);
  if (ids.length) await browser.tabs.discard(ids);
}

connect();
