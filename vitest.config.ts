import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Tests del frontend (Ajustes). La UI solo habla con el backend por `src/api.ts`, que en los
// tests se reemplaza por un mock (src/test/apiMock.ts): no hace falta Tauri ni Windows.
export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
    setupFiles: ["src/test/setup.ts"],
    restoreMocks: true,
    coverage: {
      provider: "v8",
      include: ["src/**/*.{ts,tsx}"],
      exclude: ["src/**/*.test.{ts,tsx}", "src/test/**", "src/main.tsx", "src/types.ts"],
      reporter: ["text", "html"],
      reportsDirectory: "coverage/frontend",
    },
  },
});
