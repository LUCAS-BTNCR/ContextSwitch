import { useRef, useState, type ReactNode } from "react";

/**
 * Envuelve una opción y muestra su descripción en un globito tras dejar el mouse
 * `delay` ms encima (o al enfocar el ⓘ con el teclado). Sin texto fijo debajo.
 */
export function HoverInfo({ text, delay = 400, children }: { text: string; delay?: number; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const timer = useRef<number>();
  const show = () => { window.clearTimeout(timer.current); timer.current = window.setTimeout(() => setOpen(true), delay); };
  const hide = () => { window.clearTimeout(timer.current); setOpen(false); };

  return (
    <div className="hover-info" onMouseEnter={show} onMouseLeave={hide}>
      {children}
      <span className="info-dot" tabIndex={0} role="img" aria-label={text}
            onFocus={() => setOpen(true)} onBlur={hide}>i</span>
      {open && <div className="info-tip" role="tooltip">{text}</div>}
    </div>
  );
}
