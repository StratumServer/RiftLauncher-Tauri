import { resolve } from "path"
import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"
import tailwindcss from "@tailwindcss/vite"

/**
 * Builds the renderer on its own.
 *
 * The Electron build had three roots to configure (main, preload, renderer);
 * the Tauri host is a Rust binary built by cargo, so all that is left here is
 * the front. Root is src/renderer because index.html lives there and loads
 * /src/main.tsx relative to itself, exactly as it did before.
 *
 * The aliases are the two the renderer actually uses. @src is gone with the
 * Electron host it pointed at.
 */
export default defineConfig({
  root: resolve(__dirname, "src/renderer"),
  // Tauri serves the bundle from the filesystem root of its own protocol, so assets
  // are referenced relatively rather than from "/".
  base: "./",
  build: {
    outDir: resolve(__dirname, "dist"),
    emptyOutDir: true,
    rollupOptions: {
      external: ["*.json"]
    }
  },
  resolve: {
    alias: {
      "@renderer": resolve(__dirname, "src/renderer/src"),
      "@domain": resolve(__dirname, "src/domain")
    }
  },
  // Fixed port with strictPort: cargo tauri dev points its window at this address and
  // must fail loudly rather than open a blank window when something else holds it.
  server: { port: 5173, strictPort: true },
  plugins: [react(), tailwindcss()]
})
