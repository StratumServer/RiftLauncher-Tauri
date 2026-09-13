import { resolve } from "path"
import { defineConfig } from "vitest/config"
import react from "@vitejs/plugin-react"

export default defineConfig({
  test: {
    projects: [
      {
        extends: true,
        resolve: {
          alias: {
            "@domain": resolve(__dirname, "src/domain")
          }
        },
        test: {
          name: "node",
          environment: "node",
          include: ["tests/**/*.test.ts"]
        }
      },
      {
        extends: true,
        plugins: [react()],
        resolve: {
          alias: {
            "@renderer": resolve(__dirname, "src/renderer/src"),
            "@domain": resolve(__dirname, "src/domain")
          }
        },
        test: {
          name: "renderer-dom",
          environment: "jsdom",
          include: ["tests/renderer-dom/**/*.test.tsx"],
          setupFiles: ["tests/renderer-dom/setup.ts"],
          // A test that types a form through userEvent runs every keystroke through the
          // event pipeline, and on a loaded Windows runner one of them crossed the 5 s
          // default while the whole suite ran beside it. Headroom, not slack: a hang
          // still fails, three times later.
          testTimeout: 15_000
        }
      }
    ],
    coverage: {
      provider: "v8",
      // Default reporters (text, html, clover, json) plus lcov for SonarCloud.
      reporter: ["text", "html", "clover", "json", "lcov"],
      // Renderer logic and the domain. Presentation stays out on purpose
      // (pages/**, components/**, App.tsx, main.tsx, i18n.ts): the DOM
      // harness exercises it through behavior, but line-covering JSX is
      // theater, not a signal worth gating on.
      include: [
        "src/domain/**",
        "src/renderer/src/adapters/**",
        "src/renderer/src/host/**",
        "src/renderer/src/hooks/**",
        "src/renderer/src/utils/**",
        "src/renderer/src/contexts/**",
        "src/renderer/src/features/**/hooks/**",
        "src/renderer/src/features/**/adapters/**",
        "src/renderer/src/features/config/contexts/**",
        "src/renderer/src/features/config/utils/**"
      ]
      // No thresholds yet. The floors this file carried were measured against a tree
      // that still had the Electron host in it, and every one of those numbers is now
      // about code that is gone. New floors get measured once the host commands stop
      // being stubs; gating on stale numbers would only pin the wrong thing.
    }
  }
})
