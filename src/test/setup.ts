import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

// Desmontar lo renderizado entre tests (aislamiento).
afterEach(() => cleanup());
