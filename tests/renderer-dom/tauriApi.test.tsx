/**
 * The bridge between the renderer and the Rust host.
 *
 * Three things are worth pinning here and nothing else is: that a real call
 * carries what the host expects and validates what comes back, that a stub
 * refuses in its own vocabulary instead of inventing a success, and that it says
 * so once rather than on every call.
 *
 * It lives in the renderer-dom project because the bridge writes to `window`,
 * which the node project has none of.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

const invoke = vi.fn()
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]): unknown => invoke(...args) }))

const FOLDERS = {
  defaultInstallationsFolder: "/home/player/.config/RiftLauncherInstallations",
  defaultVersionsFolder: "/home/player/.config/RiftLauncherGameVersions",
  backupsFolder: "/home/player/.config/RiftLauncherBackups"
}

async function bridge(): Promise<typeof import("@renderer/host/tauriApi")> {
  return import("@renderer/host/tauriApi")
}

beforeEach(() => {
  invoke.mockReset()
  vi.resetModules()
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe("reading and writing the config", () => {
  it("validates whatever the host hands over instead of trusting it", async () => {
    invoke.mockResolvedValue({ document: { installations: [{ id: "a", path: "/games/a", name: 42 }], favMods: "not a list" }, folders: FOLDERS })

    const config = await (await bridge()).tauriApi.configManager.getConfig()

    expect(config.installations).toHaveLength(1)
    // A field the document got wrong comes back as the default, not as 42.
    expect(config.installations[0]?.name).toBe("")
    expect(config.favMods).toEqual([])
    expect(config.backupsFolder).toBe(FOLDERS.backupsFolder)
  })

  it("falls back to a default config when the host cannot answer", async () => {
    invoke.mockRejectedValue(new Error("no config directory"))
    vi.spyOn(console, "error").mockImplementation(() => {})

    const config = await (await bridge()).tauriApi.configManager.getConfig()

    expect(config.installations).toEqual([])
    expect(config.gameVersions).toEqual([])
  })

  it("strips the session-only fields on the way to disk", async () => {
    const api = (await bridge()).tauriApi
    invoke.mockResolvedValueOnce({ document: {}, folders: FOLDERS })
    const config = await api.configManager.getConfig()

    invoke.mockResolvedValueOnce(undefined)
    const saved = await api.configManager.saveConfig({
      ...config,
      _backgroundRevision: 7,
      gameVersions: [{ id: "v1", version: "1.20.0", label: "1.20.0", path: "/games/v1", _installing: true }]
    })

    expect(saved).toEqual({ ok: true })
    const [command, payload] = invoke.mock.calls[1] as [string, { document: Record<string, unknown> }]
    expect(command).toBe("write_config")
    expect(payload.document).not.toHaveProperty("_backgroundRevision")
    expect(payload.document.gameVersions).toEqual([{ id: "v1", version: "1.20.0", label: "1.20.0", path: "/games/v1" }])
  })

  it("reports a refused write rather than claiming it landed", async () => {
    const api = (await bridge()).tauriApi
    invoke.mockResolvedValueOnce({ document: {}, folders: FOLDERS })
    const config = await api.configManager.getConfig()

    vi.spyOn(console, "error").mockImplementation(() => {})
    invoke.mockRejectedValueOnce(new Error("read-only filesystem"))

    expect(await api.configManager.saveConfig(config)).toEqual({ ok: false, reason: "write-failed" })
  })
})

describe("starting the game", () => {
  it("sends the two ids and nothing else", async () => {
    invoke.mockResolvedValue({ ok: true, exitCode: 0 })

    const result = await (
      await bridge()
    ).tauriApi.gameManager.executeGame({ id: "version-1", version: "1.20.0", label: "1.20.0", path: "/games/v1" }, { id: "install-1", path: "/games/i1" } as InstallationType)

    expect(invoke).toHaveBeenCalledWith("launch_game", { versionId: "version-1", installationId: "install-1" })
    expect(result).toEqual({ ok: true, exitCode: 0 })
  })

  it("turns a failed call into a refusal the renderer already knows how to read", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {})
    invoke.mockRejectedValue(new Error("the command panicked"))

    const result = await (await bridge()).tauriApi.gameManager.executeGame({ id: "v", version: "1", label: "1", path: "/p" }, { id: "i", path: "/i" } as InstallationType)

    expect(result).toEqual({ ok: false, reason: "launch-failed" })
  })
})

describe("the stubs", () => {
  it("refuses in each member's own vocabulary and never invents a success", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => {})
    const api = (await bridge()).tauriApi

    expect(await api.pathsManager.checkPathExists("/anything")).toBe(false)
    expect(await api.pathsManager.runInstaller("id", "/f", "/o", false)).toEqual({ ok: false, reason: "installer-failed" })
    expect(await api.modsManager.getInstalledMods("/mods")).toEqual({ mods: [], errors: [], unreadable: true })
    expect(await api.netManager.fetchReleaseNotes()).toEqual({ ok: false, reason: "offline" })
    expect(await api.accountManager.login("player@example.test", "secret")).toEqual({ status: "unexpected-response" })
    expect(await api.backgroundsManager.ensureBackground("id", "file.jpg")).toBe("failed")
    expect(invoke).not.toHaveBeenCalled()
  })

  it("says a member is missing once, not on every call", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {})
    const api = (await bridge()).tauriApi

    await api.pathsManager.checkPathExists("/a")
    await api.pathsManager.checkPathExists("/b")
    await api.pathsManager.checkPathEmpty("/c")

    const lines = warn.mock.calls.map(([line]) => String(line))
    expect(lines.filter((line) => line.includes("pathsManager.checkPathExists"))).toHaveLength(1)
    expect(lines.filter((line) => line.includes("pathsManager.checkPathEmpty"))).toHaveLength(1)
    expect(lines.every((line) => line.startsWith("[host] [tauriApi] not-implemented-on-tauri"))).toBe(true)
  })

  it("hands back an unsubscribe that can be called", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => {})
    const api = (await bridge()).tauriApi

    expect(() => api.pathsManager.onDownloadProgress(() => {})()).not.toThrow()
  })
})

describe("installing the bridge", () => {
  it("leaves an existing window.api alone so a test harness keeps its own", async () => {
    const mock = { marker: true } as unknown as BridgeAPI
    window.api = mock

    const { installTauriApi } = await bridge()
    installTauriApi()

    expect(window.api).toBe(mock)
  })
})
