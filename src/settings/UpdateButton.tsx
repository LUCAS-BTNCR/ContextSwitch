import { useEffect, useState } from "react";
import { api } from "../api";
import type { UpdateInfo } from "../types";

type State =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "uptodate"; info: UpdateInfo }
  | { kind: "available"; info: UpdateInfo }
  | { kind: "installing"; info: UpdateInfo }
  | { kind: "error"; message: string };

/** "Buscar actualizaciones": consulta la última Release publicada y, si hay una nueva, la instala. */
export function UpdateButton() {
  const [version, setVersion] = useState("");
  const [state, setState] = useState<State>({ kind: "idle" });

  useEffect(() => { api.appVersion().then(setVersion).catch(() => {}); }, []);

  const check = () => {
    setState({ kind: "checking" });
    api.checkUpdate()
      .then((info) => setState(info.available ? { kind: "available", info } : { kind: "uptodate", info }))
      .catch((e) => setState({ kind: "error", message: String(e) }));
  };

  const install = (info: UpdateInfo) => {
    if (!info.installerUrl) return;
    setState({ kind: "installing", info });
    // Si sale bien, la app se cierra y vuelve a abrirse ya actualizada.
    api.installUpdate(info.installerUrl).catch((e) => setState({ kind: "error", message: String(e) }));
  };

  return (
    <div className="update-box">
      {state.kind === "available" ? (
        <>
          <span className="update-msg new">Nueva versión v{state.info.latest} (tenés v{state.info.current})</span>
          <button className="btn primary wide" onClick={() => install(state.info)}>Actualizar a v{state.info.latest}</button>
        </>
      ) : state.kind === "installing" ? (
        <span className="update-msg">Descargando v{state.info.latest}… la app se va a reiniciar sola.</span>
      ) : (
        <>
          <button className="btn ghost wide" onClick={check} disabled={state.kind === "checking"}>
            {state.kind === "checking" ? "Buscando…" : "Buscar actualizaciones"}
          </button>
          {state.kind === "uptodate" && <span className="update-msg ok">Estás al día · v{state.info.current}</span>}
          {state.kind === "error" && <span className="update-msg err">{state.message}</span>}
          {state.kind === "idle" && version && <span className="update-msg">Versión v{version}</span>}
        </>
      )}
    </div>
  );
}
