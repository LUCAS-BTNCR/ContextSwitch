import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import { api } from "./api";
import { initPlatform } from "./platform";
import { StartupPicker } from "./StartupPicker";

// Misma página para las dos ventanas: "picker" = ventanita del inicio del sistema.
const isPicker = getCurrentWindow().label === "picker";

initPlatform(api.platformName).then(() =>
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      {isPicker ? <StartupPicker /> : <App />}
    </StrictMode>,
  ));
