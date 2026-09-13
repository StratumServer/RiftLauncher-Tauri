/**
 * Reads a stored config document into the shape the launcher runs on.
 *
 * This is the validation the Electron host did inside `src/config/configManager.ts`,
 * lifted into the domain unchanged. Nothing about it was ever host work: every rule
 * here is a decision about what a config field may hold, and it is now the only
 * boundary check the config has, because the Tauri host does file IO and nothing
 * else with this document.
 *
 * The one thing the host still has to answer is the three folder paths a fresh
 * config falls back to, which come from the OS config directory. They arrive as
 * {@link ConfigFolderDefaults} rather than being read here, which is what keeps
 * this module pure and testable on any machine.
 */

import { toPublicAccount } from "../account/credentials"
import { normalizeAccentColorId } from "../accentColors"
import { normalizeBackgroundId } from "../backgrounds"
import { normalizeModDbVisibilityAnswer } from "../moddbVisibility"
import { normalizeReceiveBetaUpdates } from "../appUpdate/betaUpdates"
import { clampConfigSchema, CURRENT_CONFIG_SCHEMA, isRecord, isUsableGameVersion, repairGameVersionIdentity } from "./migrations"
import { DEFAULT_COMPRESSION_LEVEL, DEFAULT_CONFIG_BASE } from "./defaults"

/** The three roots only the host can name, since they hang off the OS config directory. */
export type ConfigFolderDefaults = Pick<ConfigType, "defaultInstallationsFolder" | "defaultVersionsFolder" | "backupsFolder">

/** The config a host with no `config.json` yet should write. */
export function defaultConfig(folders: ConfigFolderDefaults): ConfigType {
  return { ...DEFAULT_CONFIG_BASE, schemaVersion: CURRENT_CONFIG_SCHEMA, ...folders }
}

const defaultInstallation = {
  backupsLimit: 3,
  backupsAuto: false,
  compressionLevel: DEFAULT_COMPRESSION_LEVEL,
  lastTimePlayed: -1,
  totalTimePlayed: 0,
  mesaGlThread: false
} as const

function asString(value: unknown, fallback: string, maxLength = 4_096): string {
  return typeof value === "string" && value.length <= maxLength && !value.includes("\0") ? value : fallback
}

function asNumber(value: unknown, fallback: number, min: number, max: number): number {
  return typeof value === "number" && Number.isFinite(value) && value >= min && value <= max ? value : fallback
}

function asBoolean(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback
}

function normalizeBackup(value: unknown): BackupType | null {
  if (!isRecord(value)) return null
  const id = asString(value.id, "", 128)
  const path = asString(value.path, "")
  if (!id || !path) return null
  return {
    id,
    date: asNumber(value.date, 0, 0, Number.MAX_SAFE_INTEGER),
    path
  }
}

function normalizeInstallation(value: unknown): InstallationType | null {
  if (!isRecord(value)) return null
  const installation: InstallationType = {
    id: asString(value.id, "", 128),
    name: asString(value.name, "", 256),
    icon: asString(value.icon, "", 256),
    path: asString(value.path, ""),
    version: asString(value.version, "", 128),
    gameVersionId: typeof value.gameVersionId === "string" && value.gameVersionId.length > 0 && value.gameVersionId.length <= 128 ? value.gameVersionId : null,
    startParams: asString(value.startParams, "", 8_192),
    backupsLimit: asNumber(value.backupsLimit, defaultInstallation.backupsLimit, 0, 100),
    backupsAuto: asBoolean(value.backupsAuto, defaultInstallation.backupsAuto),
    compressionLevel: Math.trunc(asNumber(value.compressionLevel, defaultInstallation.compressionLevel, 0, 9)),
    backups: Array.isArray(value.backups)
      ? value.backups
          .map(normalizeBackup)
          .filter((backup): backup is BackupType => backup !== null)
          .slice(0, 100)
      : [],
    lastTimePlayed: asNumber(value.lastTimePlayed, defaultInstallation.lastTimePlayed, -1, Number.MAX_SAFE_INTEGER),
    totalTimePlayed: asNumber(value.totalTimePlayed, defaultInstallation.totalTimePlayed, 0, Number.MAX_SAFE_INTEGER),
    mesaGlThread: asBoolean(value.mesaGlThread, defaultInstallation.mesaGlThread),
    envVars: asString(value.envVars, "", 8_192)
  }

  const launchWrapper = asString(value.launchWrapper, "", 4_096).trim()
  if (launchWrapper) installation.launchWrapper = launchWrapper

  return installation.id && installation.path ? installation : null
}

function normalizeGameVersion(value: unknown): GameVersionType | null {
  if (!isUsableGameVersion(value)) return null
  const gameVersion: GameVersionType = {
    id: asString(value.id, "", 128),
    version: value.version,
    label: asString(value.label, "", 256) || asString(value.version, "", 128),
    path: value.path
  }
  // Only set when true so a plain version, or an unset one, doesn't grow a `linked: false`
  // it never had. This flag is what keeps a player's own install off the delete path, so
  // dropping it silently on the next load would turn "remove from list" back into deletion.
  if (asBoolean(value.linked, false)) gameVersion.linked = true
  return gameVersion
}

function normalizeIcon(value: unknown): IconType | null {
  if (!isRecord(value)) return null
  const icon: IconType = {
    id: asString(value.id, "", 128),
    name: asString(value.name, "", 256),
    icon: asString(value.icon, "", 4_096),
    custom: value.custom === true
  }
  return icon.id && icon.name && icon.icon.toLowerCase().endsWith(".png") ? icon : null
}

/** Ceiling on saved accounts, the same shape as the 1,000-entry caps below: generous for the real use case, not a promise to scale past it. */
const MAX_STORED_ACCOUNTS = 50

/** Reads the accounts list, dropping anything unreadable and deduplicating by `playerUid`. */
function normalizeAccounts(value: unknown): AccountPublicType[] {
  const seen = new Set<string>()
  return (Array.isArray(value) ? value : [])
    .map(toPublicAccount)
    .filter((account): account is AccountPublicType => account !== null)
    .filter((account) => {
      if (seen.has(account.playerUid)) return false
      seen.add(account.playerUid)
      return true
    })
    .slice(0, MAX_STORED_ACCOUNTS)
}

export function normalizeConfig(config: unknown, folders: ConfigFolderDefaults): ConfigType {
  const fallback = defaultConfig(folders)
  const repairedConfig = repairGameVersionIdentity(config)
  const rawConfig = (isRecord(repairedConfig) ? repairedConfig : {}) as Partial<ConfigType>
  const rawWindow = (isRecord(rawConfig.window) ? rawConfig.window : {}) as Partial<WindowType>
  const installations = (Array.isArray(rawConfig.installations) ? rawConfig.installations : [])
    .map(normalizeInstallation)
    .filter((installation): installation is InstallationType => installation !== null)
    .slice(0, 1_000)

  const gameVersions = (Array.isArray(rawConfig.gameVersions) ? rawConfig.gameVersions : [])
    .map(normalizeGameVersion)
    .filter((gameVersion): gameVersion is GameVersionType => gameVersion !== null)
    .slice(0, 1_000)

  const customIcons = (Array.isArray(rawConfig.customIcons) ? rawConfig.customIcons : [])
    .map(normalizeIcon)
    .filter((icon): icon is IconType => icon !== null)
    .slice(0, 1_000)

  const accounts = normalizeAccounts(rawConfig.accounts)

  return {
    schemaVersion: clampConfigSchema(rawConfig.schemaVersion),
    lastUsedInstallation: rawConfig.lastUsedInstallation === null ? null : asString(rawConfig.lastUsedInstallation, fallback.lastUsedInstallation ?? "", 128) || null,
    defaultInstallationsFolder: asString(rawConfig.defaultInstallationsFolder, fallback.defaultInstallationsFolder),
    defaultVersionsFolder: asString(rawConfig.defaultVersionsFolder, fallback.defaultVersionsFolder),
    backupsFolder: asString(rawConfig.backupsFolder, fallback.backupsFolder),
    window: {
      width: Math.trunc(asNumber(rawWindow.width, fallback.window.width, 1_024, 8_192)),
      height: Math.trunc(asNumber(rawWindow.height, fallback.window.height, 600, 8_192)),
      x: Math.trunc(asNumber(rawWindow.x, fallback.window.x, -100_000, 100_000)),
      y: Math.trunc(asNumber(rawWindow.y, fallback.window.y, -100_000, 100_000)),
      maximized: asBoolean(rawWindow.maximized, fallback.window.maximized)
    },
    accounts,
    // An id naming nobody falls back to the first saved account rather than to null: a
    // household that lost its choice (a dangling id, or a config hand-edited down to one
    // fewer account) should land on someone, not on "no account selected". Every reader
    // downstream can then do a plain lookup with no fallback branch of its own, because
    // this is the one place the invariant is enforced: activeAccountId either names an
    // entry in accounts, or is null when accounts is empty.
    activeAccountId: accounts.some((account) => account.playerUid === rawConfig.activeAccountId) ? (rawConfig.activeAccountId as string) : (accounts[0]?.playerUid ?? null),
    installations,
    gameVersions,
    favMods: Array.isArray(rawConfig.favMods) ? rawConfig.favMods.filter((modId): modId is number => typeof modId === "number" && Number.isSafeInteger(modId)).slice(0, 10_000) : fallback.favMods,
    suspendedModUpdates: Array.isArray(rawConfig.suspendedModUpdates)
      ? rawConfig.suspendedModUpdates.filter((modid): modid is string => typeof modid === "string" && modid.length > 0).slice(0, 10_000)
      : fallback.suspendedModUpdates,
    // A stored id survives whether or not the catalog still lists it: the manifest is not
    // readable from here, and a scene retired from the branch for a week should not silently
    // reset a player's choice. Anything that is not a usable id falls back to the bundled scene,
    // which is also what the renderer paints when the cached file for an id has gone missing.
    background: normalizeBackgroundId(rawConfig.background),
    // Anything that does not name a listed preset, missing included, becomes the shipped default:
    // a config written before this field existed paints exactly as it always has.
    accentColor: normalizeAccentColorId(rawConfig.accentColor),
    // Anything unreadable becomes "not asked yet", which costs one question and never invents a
    // consent. The prompt is the only thing that ever writes a real answer here.
    moddbVisibilityAnswer: normalizeModDbVisibilityAnswer(rawConfig.moddbVisibilityAnswer),
    // Null for anything that is not an explicit yes or no, which is what every config written
    // before the toggle existed says, and leaves the running version deciding as it always did.
    receiveBetaUpdates: normalizeReceiveBetaUpdates(rawConfig.receiveBetaUpdates),
    // Empty for anything unreadable, a config written before this field existed included: the
    // "what's new" dialog reads that the same way it reads a fresh install, showing only the
    // running version's own notes rather than guessing at a history it was never told.
    lastSeenChangelogVersion: asString(rawConfig.lastSeenChangelogVersion, fallback.lastSeenChangelogVersion, 128),
    customIcons
  }
}

/**
 * Strips the session-only underscore fields before a config is written.
 *
 * `_installing`, `_playing`, `_backgroundRevision` and the rest describe what the
 * running app is doing, not what it is configured to do. The Electron host dropped
 * them on the way to disk and so does this.
 */
export function toStorableConfig(config: ConfigType): unknown {
  return JSON.parse(JSON.stringify(config, (key, value) => (key.startsWith("_") ? undefined : value)))
}
