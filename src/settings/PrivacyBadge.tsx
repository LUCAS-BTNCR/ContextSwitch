import { useState } from "react";

const GUARANTEES = [
  ["Sin red", "La app no tiene permisos de red: la ventana bloquea toda conexión (CSP) y el backend no incluye cliente HTTP."],
  ["Sin cuentas", "Nunca pide contraseñas, tokens ni cookies, ni lee bases de datos de correo, chat o navegador."],
  ["Sin registro de teclas", "El hook compara cada tecla con tus combinaciones y la descarta al instante. No hay historial."],
  ["Un solo archivo", "Todo vive en %APPDATA%\\ContextSwitch\\profiles.json. Borralo y no queda rastro."],
  ["Cierres limpios", "Se cierra como si hicieras clic en la X: si hay cambios sin guardar, la app te pregunta."],
] as const;

export function PrivacyBadge({ compact = false }: { compact?: boolean }) {
  const [open, setOpen] = useState(false);
  if (compact) {
    return (
      <button className="privacy-pill" onClick={() => setOpen((o) => !o)} aria-expanded={open}>
        <span className="dot" /> 100% local · sin red · sin cuentas
        {open && (
          <span className="privacy-pop" role="dialog">
            {GUARANTEES.map(([t, d]) => <span key={t}><b>{t}</b>{d}</span>)}
          </span>
        )}
      </button>
    );
  }
  return (
    <section className="card privacy">
      <header className="card-head">
        <span className="step">4</span>
        <div>
          <h2>Garantía de privacidad</h2>
          <p className="muted">Lo que ContextSwitch hace y, sobre todo, lo que no hace.</p>
        </div>
      </header>
      <ul className="guarantees">
        {GUARANTEES.map(([t, d]) => (
          <li key={t}><span className="check">✓</span><div><b>{t}</b><p className="muted">{d}</p></div></li>
        ))}
      </ul>
    </section>
  );
}
