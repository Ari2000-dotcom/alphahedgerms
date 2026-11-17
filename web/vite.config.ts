import { defineConfig } from "vite";
import preact from "@preact/preset-vite";

// In development the page is on :5290 and its /api calls go to the server on :8090, so the
// admin's session cookie stays same-origin. In production the server serves the built files itself.
export default defineConfig({
  plugins: [preact()],
  clearScreen: false,
  server: { port: 5290, strictPort: true, proxy: { "/api": "http://127.0.0.1:8090" } },
  build: { target: "es2022", cssCodeSplit: false, reportCompressedSize: false },
});
