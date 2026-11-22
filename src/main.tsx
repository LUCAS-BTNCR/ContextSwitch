import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import { StartupPicker } from "./StartupPicker";

// Misma página para las dos ventanas: "picker" = ventanita del inicio con Windows.
const isPicker = getCurrentWindow().label === "picker";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {isPicker ? <StartupPicker /> : <App />}
  </StrictMode>,
);
