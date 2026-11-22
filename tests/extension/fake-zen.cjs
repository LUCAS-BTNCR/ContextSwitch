// Zen simulado para testear extension-zen/lib.js sin navegador.
// Reproduce el comportamiento que importa: una ventana NUEVA trae copias de los
// Essentials (pestañas fijadas) además de su pestaña inicial — eso fue lo que hizo que
// una versión vieja cerrara Essentials al cerrar "la primera pestaña".

class FakeZen {
  constructor({ containers = ["Personal", "Work"], essentials = [] } = {}) {
    this.nextId = 1;
    this.containers = containers.map((name, i) => ({ name, cookieStoreId: `firefox-container-${i + 1}` }));
    this.essentials = essentials; // [{ url, container }] que aparecen en cada ventana nueva
    this.windows = new Map(); // id -> { id, focused }
    this.tabs = new Map(); // id -> tab
    this.removed = []; // ids cerrados, en orden
    this.discarded = [];
    this.created = []; // ids creados por la extensión vía tabs.create
    this.storageData = {};
    this.closed = []; // sessions.getRecentlyClosed
    this.restoredIds = [];
    this.removedListeners = [];
    this.failCreateContainer = false;
    this.failTabsCreateFor = null; // url que hace fallar tabs.create
    this.browser = this.#api();
  }

  csid(container) { return this.containers.find((c) => c.name === container)?.cookieStoreId; }

  addWindow() {
    const id = this.nextId++;
    this.windows.set(id, { id, focused: false });
    return id;
  }

  addTab(props) {
    const tab = { id: this.nextId++, active: false, pinned: false, discarded: false, audible: false,
                  lastAccessed: 0, cookieStoreId: "firefox-default", url: "about:blank", ...props };
    this.tabs.set(tab.id, tab);
    return tab;
  }

  pinnedIds() { return [...this.tabs.values()].filter((t) => t.pinned).map((t) => t.id); }

  #match(t, q) {
    return Object.entries(q).every(([k, v]) => t[k] === v);
  }

  #api() {
    const z = this;
    const copy = (t) => ({ ...t });
    return {
      contextualIdentities: {
        query: async () => z.containers.map(copy),
        create: async ({ name }) => {
          if (z.failCreateContainer) throw new Error("no se pudo crear");
          const c = { name, cookieStoreId: `firefox-container-${z.containers.length + 1}` };
          z.containers.push(c);
          return copy(c);
        },
      },
      windows: {
        create: async ({ url }) => {
          const windowId = z.addWindow();
          // Como Zen: copias de los Essentials + la pestaña inicial pedida.
          for (const e of z.essentials) z.addTab({ windowId, url: e.url, pinned: true, cookieStoreId: z.csid(e.container) });
          z.addTab({ windowId, url: url ?? "about:newtab", active: true });
          return { id: windowId, tabs: [...z.tabs.values()].filter((t) => t.windowId === windowId).map(copy) };
        },
        getAll: async () => [...z.windows.values()].map(copy),
        update: async (id, props) => { Object.assign(z.windows.get(id), props); return copy(z.windows.get(id)); },
        onRemoved: { addListener: (fn) => z.removedListeners.push(fn) },
      },
      tabs: {
        create: async ({ windowId, url, cookieStoreId, active }) => {
          if (url && url === z.failTabsCreateFor) throw new Error(`Illegal URL: ${url}`);
          const wid = windowId ?? [...z.windows.keys()][0] ?? z.addWindow();
          const t = z.addTab({ windowId: wid, url: url ?? "about:newtab", cookieStoreId: cookieStoreId ?? "firefox-default", active: !!active });
          z.created.push(t.id);
          return copy(t);
        },
        query: async (q = {}) => [...z.tabs.values()].filter((t) => z.#match(t, q)).map(copy),
        get: async (id) => { if (!z.tabs.has(id)) throw new Error(`Invalid tab ID: ${id}`); return copy(z.tabs.get(id)); },
        update: async (id, props) => {
          if (!z.tabs.has(id)) throw new Error(`Invalid tab ID: ${id}`);
          if (props.active) for (const t of z.tabs.values()) if (t.windowId === z.tabs.get(id).windowId) t.active = false;
          Object.assign(z.tabs.get(id), props);
          return copy(z.tabs.get(id));
        },
        remove: async (id) => {
          if (!z.tabs.has(id)) throw new Error(`Invalid tab ID: ${id}`);
          z.tabs.delete(id);
          z.removed.push(id);
        },
        discard: async (id) => {
          if (!z.tabs.has(id)) throw new Error(`Invalid tab ID: ${id}`);
          z.tabs.get(id).discarded = true;
          z.discarded.push(id);
        },
      },
      storage: {
        local: {
          get: async (key) => (key in z.storageData ? { [key]: z.storageData[key] } : {}),
          set: async (obj) => { Object.assign(z.storageData, obj); },
        },
      },
      sessions: {
        getRecentlyClosed: async () => z.closed.map((c) => JSON.parse(JSON.stringify(c))),
        restore: async (sessionId) => {
          const item = z.closed.find((c) => c.tab?.sessionId === sessionId);
          if (!item) throw new Error("no existe");
          if (item.failRestore) throw new Error("falló restore");
          z.restoredIds.push(sessionId);
          return { tab: { ...item.tab, pinned: !!item.tab.pinned } };
        },
      },
    };
  }
}

module.exports = { FakeZen };
