import { useEffect, useState } from "react";
import { api } from "../api";
import { isMac } from "../platform";

/**
 * Mac: sin el permiso de Accesibilidad no funciona el atajo de teclado ni se pueden
 * mover, minimizar o reconocer ventanas. Se avisa hasta que el usuario lo concede
 * (se vuelve a consultar cada 2 s: el atajo arranca solo apenas se activa).
 */
export function AccessibilityBanner() {
  const [ok, setOk] = useState(true);
  useEffect(() => {
    if (!isMac()) return;
    let alive = true;
    const check = () => api.accessibilityOk().then((v) => alive && setOk(v)).catch(() => {});
    check();
    const t = window.setInterval(check, 2000);
    return () => { alive = false; window.clearInterval(t); };
  }, []);
  if (ok) return null;
  return (
    <div className="warn activation-warn" role="alert">
      <div>
        <b>Falta el permiso de Accesibilidad.</b> Sin él no funciona el atajo de teclado ni se pueden ubicar,
        minimizar o reconocer ventanas. ContextSwitch no lee lo que escribís: solo reacciona a sus atajos.
      </div>
      <button className="btn small" onClick={() => api.openAccessibilitySettings()}>Abrir Ajustes del Sistema</button>
    </div>
  );
}
