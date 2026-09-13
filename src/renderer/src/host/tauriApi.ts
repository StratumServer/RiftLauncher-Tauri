/**
 * The Tauri side of `window.api`.
 *
 * The renderer is the part of RiftLauncher this repository is not changing, so
 * the bridge keeps exactly the shape `src/api.d.ts` declares and the renderer
 * keeps calling it exactly as it did under Electron. What changed is what sits
 * behind each member.
 *
 * Three of them reach the Rust host: reading the config, writing it back, and
 * starting the game. Everything else is a stub, and a stub here means one thing
 * only: it refuses in the vocabulary its own return type already has, and says
 * so once in the console under a fixed token. It never invents a success. A
 * `checkPathExists` that answered `true` because answering something felt more
 * useful would have the renderer build a flow on top of a folder that is not
 * there, and the evaluation would be reading a bug rather than a gap.
 *
 * Two members are answered here rather than stubbed, because the answer never
 * needed a host in the first place: `getOs` reads the user agent, and
 * `logMessage` writes to the console. Both are marked below.
 */

import { invoke } from "@tauri-apps/api/core"
import { normalizeConfig, toStorableConfig, type ConfigFolderDefaults } from "@domain/config/normalize"

/** The token every stub logs under, so a grep of a bug report finds all of them at once. */
const STUB_TOKEN = "[host] [tauriApi] not-implemented-on-tauri"

const announced = new Set<string>()

/**
 * Answers for a member the Tauri host does not implement yet, saying so once.
 *
 * Once, not once per call: `checkPathExists` runs on every keystroke of a folder
 * field, and a console full of the same line is a console nobody reads.
 */
function stub<T>(member: string, refusal: T): T {
  if (!announced.has(member)) {
    announced.add(member)
    console.warn(`${STUB_TOKEN} ${member}`)
  }
  return refusal
}

/** The same, for a member that answers a promise. */
function stubAsync<T>(member: string, refusal: T): Promise<T> {
  return Promise.resolve(stub(member, refusal))
}

/** A subscription to events the host never sends. Unsubscribing is a no-op for the same reason. */
function stubSubscription(member: string): Unsubscribe {
  return stub(member, () => {})
}

/** What `read_config` answers. The document is null for a config that is missing or unreadable. */
type StoredConfig = {
  document: unknown
  folders: ConfigFolderDefaults
}

/**
 * The fallback folders the host named the last time the config was read.
 *
 * `saveConfig` normalizes before writing and normalizing needs them, so the
 * alternative would be a second `read_config` on every save. The renderer always
 * reads the config before it can change it, so by the time a save happens this
 * is filled in; the read below is the safety net for the order being different
 * one day, not the normal path.
 */
let knownFolders: ConfigFolderDefaults | null = null

async function readStoredConfig(): Promise<StoredConfig> {
  const stored = await invoke<StoredConfig>("read_config")
  knownFolders = stored.folders
  return stored
}

async function folders(): Promise<ConfigFolderDefaults> {
  return knownFolders ?? (await readStoredConfig()).folders
}

/**
 * Reads the stored config and validates it the way the launcher always has.
 *
 * The host hands over the raw document and the three fallback folders; every
 * rule about what the document may hold is `normalizeConfig`'s, which is the
 * same function the Electron host called. A read that fails outright still
 * produces a config, because the renderer has to paint something and a default
 * config is what it painted before.
 */
async function getConfig(): Promise<ConfigType> {
  try {
    const stored = await readStoredConfig()
    return normalizeConfig(stored.document, stored.folders)
  } catch (err) {
    console.error(`[host] [tauriApi] could not read the config: ${String(err)}`)
    const fallback = knownFolders ?? { defaultInstallationsFolder: "", defaultVersionsFolder: "", backupsFolder: "" }
    return normalizeConfig(null, fallback)
  }
}

/**
 * Writes the config back, normalized and with its session-only fields stripped.
 *
 * Normalizing on the way out as well as on the way in is what the Electron host
 * did, and it is what keeps a renderer bug from putting a field on disk that the
 * next read would refuse.
 */
async function saveConfig(config: ConfigType): Promise<SaveConfigResult> {
  if (typeof config !== "object" || config === null) return { ok: false, reason: "invalid-payload" }

  try {
    const document = toStorableConfig(normalizeConfig(config, await folders()))
    await invoke("write_config", { document })
    return { ok: true }
  } catch (err) {
    console.error(`[host] [tauriApi] could not write the config: ${String(err)}`)
    return { ok: false, reason: "write-failed" }
  }
}

/**
 * Starts the game and resolves when the player closes it.
 *
 * Only the two ids cross: the host reads the folders, the start parameters, the
 * environment and the wrapper out of the saved config itself, so what runs is
 * what the config says rather than what this page says.
 */
async function executeGame(version: GameVersionType, installation: InstallationType): Promise<GameExecutionResult> {
  try {
    return await invoke<GameExecutionResult>("launch_game", { versionId: version.id, installationId: installation.id })
  } catch (err) {
    console.error(`[host] [tauriApi] the launch call failed: ${String(err)}`)
    return { ok: false, reason: "launch-failed" }
  }
}

/**
 * The installations the host reads out of `config.json`.
 *
 * Not part of `window.api`: the renderer gets its installations from `getConfig`,
 * which carries the whole config anyway. This is the host command on its own, for
 * the slice that stops reading the config wholesale.
 */
export function listInstallations(): Promise<InstallationType[]> {
  return invoke<InstallationType[]>("list_installations")
}

/**
 * The platform, read off the user agent rather than off the host.
 *
 * Not a stub: the renderer only ever asks this to decide which download to offer
 * and which Linux-only settings to show, and the browser already knows. Answering
 * "linux" from a stub on a Windows machine would be a wrong answer rather than a
 * missing one.
 */
function getOs(): Promise<NodeJS.Platform> {
  const agent = navigator.userAgent
  if (agent.includes("Windows")) return Promise.resolve("win32")
  if (agent.includes("Mac OS")) return Promise.resolve("darwin")
  return Promise.resolve("linux")
}

/** Console levels for the launcher's own log levels. The host has no log file yet. */
const CONSOLE_FOR_LEVEL: Record<ErrorTypes, "error" | "warn" | "info" | "debug"> = {
  error: "error",
  warn: "warn",
  info: "info",
  debug: "debug",
  verbose: "debug"
}

export const tauriApi: BridgeAPI = {
  utils: {
    getAppVersion: () => stubAsync("utils.getAppVersion", "0.0.0"),
    getOs,
    // Not a stub either: the console is a real destination, just not a file.
    logMessage: (mode, message) => console[CONSOLE_FOR_LEVEL[mode] ?? "info"](message),
    setPreventAppClose: () => stub("utils.setPreventAppClose", undefined),
    openOnBrowser: () => stub("utils.openOnBrowser", undefined),
    selectFolderDialog: () => stubAsync("utils.selectFolderDialog", [] as string[]),
    onPreventedAppClose: () => stubSubscription("utils.onPreventedAppClose")
  },
  appUpdater: {
    onUpdateAvailable: () => stubSubscription("appUpdater.onUpdateAvailable"),
    onUpdateDownloadProgress: () => stubSubscription("appUpdater.onUpdateDownloadProgress"),
    onUpdateError: () => stubSubscription("appUpdater.onUpdateError"),
    onUpdateDownloaded: () => stubSubscription("appUpdater.onUpdateDownloaded"),
    downloadUpdate: () => stub("appUpdater.downloadUpdate", undefined),
    updateAndRestart: () => stub("appUpdater.updateAndRestart", undefined)
  },
  configManager: {
    getConfig,
    saveConfig
  },
  modsManager: {
    getInstalledMods: () => stubAsync("modsManager.getInstalledMods", { mods: [], errors: [], unreadable: true } as InstalledModsScan),
    setModEnabled: () => stubAsync("modsManager.setModEnabled", { ok: false, reason: "refused" } as SetModEnabledResult),
    cacheModImage: () => stubAsync("modsManager.cacheModImage", undefined),
    exportModpack: () => stubAsync("modsManager.exportModpack", { success: false }),
    importModpack: () => stubAsync("modsManager.importModpack", { success: false, error: STUB_TOKEN }),
    clearModIconMemoryCache: () => stub("modsManager.clearModIconMemoryCache", undefined),
    getModProfiles: () => stubAsync("modsManager.getModProfiles", { ok: false, reason: "refused" } as ModProfilesReadResult),
    saveModProfiles: () => stubAsync("modsManager.saveModProfiles", { ok: false, reason: "refused" } as ModProfilesSaveResult)
  },
  pathsManager: {
    getCurrentUserDataPath: () => stubAsync("pathsManager.getCurrentUserDataPath", ""),
    formatPath: () => stubAsync("pathsManager.formatPath", ""),
    removeFileFromPath: () => stubAsync("pathsManager.removeFileFromPath", ""),
    deletePath: () => stubAsync("pathsManager.deletePath", false),
    movePath: () => stubAsync("pathsManager.movePath", false),
    checkPathEmpty: () => stubAsync("pathsManager.checkPathEmpty", false),
    checkPathExists: () => stubAsync("pathsManager.checkPathExists", false),
    ensurePathExists: () => stubAsync("pathsManager.ensurePathExists", false),
    openPathOnFileExplorer: () => stubAsync("pathsManager.openPathOnFileExplorer", undefined),
    downloadOnPath: () => stubAsync("pathsManager.downloadOnPath", ""),
    extractOnPath: () => stubAsync("pathsManager.extractOnPath", false),
    runInstaller: () => stubAsync("pathsManager.runInstaller", { ok: false, reason: "installer-failed" } as InstallerRunResult),
    compressOnPath: () => stubAsync("pathsManager.compressOnPath", false),
    onDownloadProgress: () => stubSubscription("pathsManager.onDownloadProgress"),
    onExtractProgress: () => stubSubscription("pathsManager.onExtractProgress"),
    onCompressProgress: () => stubSubscription("pathsManager.onCompressProgress"),
    changePerms: () => stubAsync("pathsManager.changePerms", false),
    copyToIcons: () => stubAsync("pathsManager.copyToIcons", { status: false, reason: "copy-failed" } as CustomIconCopyResult)
  },
  gameManager: {
    executeGame,
    lookForAGameVersion: () => stubAsync("gameManager.lookForAGameVersion", { exists: false } as { exists: false })
  },
  netManager: {
    queryURL: () => stubAsync("netManager.queryURL", ""),
    acceptModDbVisibility: () => stubAsync("netManager.acceptModDbVisibility", false),
    fetchReleaseNotes: () => stubAsync("netManager.fetchReleaseNotes", { ok: false, reason: "offline" } as FetchReleaseNotesResult)
  },
  backgroundsManager: {
    ensureBackground: () => stubAsync("backgroundsManager.ensureBackground", "failed" as EnsureBackgroundResult),
    copyCustomBackground: () => stubAsync("backgroundsManager.copyCustomBackground", false)
  },
  accountManager: {
    login: () => stubAsync("accountManager.login", { status: "unexpected-response" } as AccountLoginResult),
    removeAccount: () => stubAsync("accountManager.removeAccount", false)
  }
}

/**
 * Puts the bridge where the renderer already looks for it.
 *
 * Assignment rather than injection is the whole point: every page, hook and
 * adapter in `src/renderer` goes on calling `window.api` with no idea which host
 * is behind it, and the renderer-dom tests go on installing their own mock over
 * the same property. An existing `window.api` is left alone, which is what makes
 * a test that set one up first win over this.
 */
export function installTauriApi(): void {
  if (window.api) return
  window.api = tauriApi
}
