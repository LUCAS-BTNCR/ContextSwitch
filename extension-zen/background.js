// ContextSwitch Companion para Zen Browser: conexión con cs-bridge (Native Messaging).
// La lógica está en lib.js (cargado antes por el manifest), testeada en Node.

const HOST = "contextswitch_bridge";
let port = null;
let retryMs = 1000;

// Diagnóstico local: cs-bridge lo guarda en %APPDATA%\ContextSwitch\bridge.log (nada sale de la PC).
function log(event, data = {}) {
  try { port?.postMessage({ type: "log", t: new Date().toISOString(), event, ...data }); } catch {}
}

const companion = ContextSwitchCompanion.create({ browser, log });

function connect() {
  port = browser.runtime.connectNative(HOST);
  port.onMessage.addListener((msg) => {
    retryMs = 1000; // el host respondió: conexión sana, se resetea el backoff
    log("received", { msgType: msg.type });
    // Recarga la extensión desde disco (sirve también cargada como temporal).
    if (msg.type === "reload") { log("reloading"); setTimeout(() => browser.runtime.reload(), 200); return; }
    companion.handle(msg).catch((e) => { console.warn("[cs]", e); log("error", { error: String(e) }); });
  });
  log("connected", { version: browser.runtime.getManifest().version });
  port.onDisconnect.addListener((p) => {
    if (p.error) console.warn("[cs] host desconectado:", p.error.message);
    port = null;
    setTimeout(connect, retryMs);
    retryMs = Math.min(retryMs * 2, 60_000);
  });
}

connect();
companion.init().catch((e) => log("init-error", { error: String(e) }));
