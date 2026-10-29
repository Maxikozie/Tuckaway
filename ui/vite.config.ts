import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 5173, strictPort: true, fs: { allow: [".."] } },
  build: { target: "es2022", outDir: "dist", emptyOutDir: true },
  test: { environment: "node", include: ["src/**/*.test.ts"] },
});
