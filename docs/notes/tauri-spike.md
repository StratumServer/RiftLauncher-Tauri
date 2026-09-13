# Tauri spike

What a Tauri v2 host costs and buys, measured against the Electron build the team ships
today. Written for issue #18 of StratumServer/RiftLauncher, against `feat/tauri-skeleton`
at 3294a40. Both sides are version 1.7.0-beta.10 of the same front end, so every runtime
difference below is the host and the web engine, never the page.

No verdict here. The numbers and the gaps are the point; the decision belongs on the issue.

## What was built

The front end is untouched. The renderer still calls `window.api` exactly as `src/api.d.ts`
declares it, and the whole port lives behind that one object.

Three members reach the Rust host. Everything else in `src/renderer/src/host/tauriApi.ts`
(256 lines) is a stub.

| Command        | Rust                                                | Replaces                   |
| -------------- | --------------------------------------------------- | -------------------------- |
| `read_config`  | `src-tauri/src/config.rs`                           | `configManager.getConfig`  |
| `write_config` | `src-tauri/src/config.rs`, `src-tauri/src/paths.rs` | `configManager.saveConfig` |
| `launch_game`  | `src-tauri/src/launch.rs`                           | `gameManager.executeGame`  |

997 lines of production Rust and 351 of tests (21 tests), against the 671 lines of
TypeScript those same three handlers occupy in `src/ipc/handlers` on the Electron side.

A stub refuses in its own return type's vocabulary and never invents a success. It logs
`[host] [tauriApi] not-implemented-on-tauri <member>` once, not once per call, so a bug
report can be grepped for the token. Two members are answered on the front rather than
stubbed, and are marked as such in the file: `getOs` reads the user agent (a stub that
answered `"linux"` on Windows would be a wrong answer, not a missing one) and `logMessage`
writes to the console.

| `window.api` group   | Members | Ported                      | Stubbed |
| -------------------- | ------- | --------------------------- | ------- |
| `pathsManager`       | 19      | 0                           | 19      |
| `modsManager`        | 8       | 0                           | 8       |
| `utils`              | 7       | 0 (2 answered on the front) | 5       |
| `appUpdater`         | 6       | 0                           | 6       |
| `netManager`         | 3       | 0                           | 3       |
| `configManager`      | 2       | 2                           | 0       |
| `gameManager`        | 2       | 1                           | 1       |
| `backgroundsManager` | 2       | 0                           | 2       |
| `accountManager`     | 2       | 0                           | 2       |
| **Total**            | **51**  | **3**                       | **46**  |

The practical consequence: this host reads and writes the config, lists installations and
starts a game that is already installed. It cannot install a version, so a fresh user
never reaches a playable state. That is the shape of a skeleton and not a defect, but it
also means everything below was measured on an app that answers most questions with a
refusal.

Two things in the ported code a reviewer should look at:

**`saveConfig` has no unauthorized-path refusal.** The Electron host compares an incoming
config against the previous one before accepting a folder outside the managed roots
(`assertConfigPathsAuthorized`). That comparison is not ported, so `unauthorized-path`
exists in `SaveConfigResult` and never fires. Worth closing before this touches real data.

**`launch_game` restates the plan rather than calling it.** Picking the executable means
listing the version folder, which the front cannot do, so `src/domain/versions/launch.ts`
is restated in Rust. Three details a player's game depends on each have a Rust test
pinning them, but a copy can drift. If it ever has to be one thing, the fix is a
`list_version_folder` command and the domain keeping the decision.

## How it was measured

AMD Ryzen 9 9900X (24 threads), kernel 6.18.49-1-MANJARO, WebKitGTK 2.52.6, GTK 3.24.52,
measured 2026-09-14. Five cold runs each, alternating sides, throwaway XDG profiles seeded
with one installation and one game version pointing at empty managed folders, so the Home
page shows an installation in both. The Electron side is the released 1.7.0-beta.10
AppImage.

Neither Xvfb nor a headless Wayland compositor exists on this machine, so the two apps did
not share one offscreen display.

| App      | Display                                                                  | What that means                                                                        |
| -------- | ------------------------------------------------------------------------ | -------------------------------------------------------------------------------------- |
| Tauri    | GTK Broadway, `broadwayd :7`, `GDK_BACKEND=broadway BROADWAY_DISPLAY=:7` | WebKitGTK paints and encodes every frame onto the Broadway socket                      |
| Electron | `--ozone-platform=headless`                                              | Chromium has no Broadway backend, and headless ozone gives it no output surface at all |

The asymmetry runs against Tauri, not for it: one side does real presentation work and the
other skips it entirely, so the Tauri figures are an upper bound on its cost and the gap on
a shared display would be at least as wide. Both ran without GPU acceleration. Installing
`xorg-server-xvfb` or `weston` removes the caveat on a re-run.

Two biases run the other way, in Tauri's favour, and both are real:

- The Electron host does work inside the measured window that the skeleton does not: it
  scans the installation for mods and runs an autoUpdater network check around the four
  second mark. Most of what the Tauri build is asked for is refused instantly by a stub.
- Electron's Home marker was polled at 20 ms over CDP, which costs it a little CPU and
  coarsens its spread. The Tauri marker was read in page by an injected script.

This is a comparison of a finished host against a skeleton. Read it as a ceiling on what
Tauri gains, not as a like-for-like.

## Startup

Time from process spawn (wall clock taken immediately before exec) to each marker.
DOMContentLoaded and first contentful paint come from the engine itself, so both sides use
the same definition. Home interactive is the moment the Play button exists.

| Metric                          | Tauri median | Tauri runs | Electron median | Electron runs |
| ------------------------------- | ------------ | ---------- | --------------- | ------------- |
| spawn to navigationStart        | **146 ms**   | 143 to 150 | **815 ms**      | 808 to 817    |
| spawn to DOMContentLoaded       | **175 ms**   | 172 to 180 | **1036 ms**     | 1027 to 1062  |
| spawn to first contentful paint | **214 ms**   | 210 to 219 | **1172 ms**     | 1165 to 1204  |
| spawn to Home interactive       | **235 ms**   | 230 to 237 | **1164 ms**     | 1161 to 1177  |

An interactive Home page in about a fifth of the time, 235 ms against 1164 ms. Most of that
is host boot rather than page work: Electron spends 815 ms before the renderer even begins
navigating, Tauri 146 ms. Once navigation starts the two engines are much closer, 357 ms to
first paint for Chromium against 68 ms for WebKitGTK on the same page. Run to run spread is
9 ms on Tauri and 39 ms on Electron.

## Memory, CPU and processes at idle

Sampled 10 seconds after spawn, summed over the whole process tree. Summing RSS across a
tree double counts pages shared between processes, so PSS is the figure to quote.

| Metric              | Tauri median | Tauri runs     | Electron median | Electron runs  |
| ------------------- | ------------ | -------------- | --------------- | -------------- |
| PSS sum             | **345.2 MB** | 344.2 to 348.3 | **408.4 MB**    | 360.2 to 410.2 |
| RSS sum             | **638.5 MB** | 637.2 to 641.3 | **808.0 MB**    | 758.9 to 811.8 |
| CPU seconds to idle | **0.50**     | 0.50 to 0.53   | **1.04**        | 1.01 to 1.05   |
| Processes at idle   | **6**        | 6              | **8**           | 8              |

Memory is the weakest part of the case. On PSS the saving is 63 MB, about 15 percent, not
the order of magnitude Tauri's reputation suggests. WebKitGTK is not a small engine: its
web process alone holds 188 MB PSS and 309 MB RSS. CPU to idle halves, but part of that is
the stubbed host skipping the mod scan and the update check.

The Tauri tree is the host, a WebKit network process, a WebKit web process, two `bwrap`
sandbox helpers and a `glycin-svg` image decoder. The Electron tree is the browser process,
the zygote and its child, a renderer, a utility process and three more helpers; its GPU
process starts and exits immediately because EGL cannot initialise in this environment.

## Bundles and installed size

| Artifact                                | Tauri                       | Electron                      |
| --------------------------------------- | --------------------------- | ----------------------------- |
| AppImage                                | 100 710 904 B (96.0 MiB)    | 125 912 658 B (120.1 MiB)     |
| deb                                     | 3 667 338 B (3.5 MiB)       | 100 057 656 B (95.4 MiB)      |
| rpm                                     | 3 668 157 B (3.5 MiB)       | 89 806 473 B (85.6 MiB)       |
| pacman                                  | not produced                | 91 978 784 B (87.7 MiB)       |
| Windows setup.exe                       | not produced                | 100.8 MiB                     |
| Host binary, stripped                   | 5 681 320 B (5.4 MiB)       | n/a                           |
| **Installed, via `--appimage-extract`** | **291 641 500 B (278 MiB)** | **295 222 308 B (281.5 MiB)** |

This is the finding that contradicts the usual pitch. Unpacked, the two AppImages are
within 4 MB of each other, because linuxdeploy bundles the entire WebKitGTK stack: 285 MB
of libraries, of which `libwebkit2gtk-4.1.so.0` alone is 94 MB, plus 38 MB of
JavaScriptCore and 33 MB of ICU data. Tauri's size advantage exists only in the distro
packages, where the deb and rpm declare a dependency on the system `libwebkit2gtk-4.1-0`
instead of carrying it: 3.5 MiB against 95.4 MiB, and 9.4 MB of payload once installed.
That advantage is paid for with a runtime dependency on whatever WebKitGTK the user's
distribution ships, which is the variable Electron exists to remove.

AppImage bundling works on this host only with `NO_STRIP=true`. linuxdeploy carries its own
old binutils, whose `strip` cannot read the `.relr.dyn` section this host's linker puts in
the system libraries it copies into the AppDir, and it fails the whole AppImage rather than
skipping the library. The CI bundle job sets the same variable with the reason next to it.

## WebKitGTK compatibility

This is the risk the spike existed to test. WebKitGTK 2.52.6
(`AppleWebKit/605.1.15 ... Version/60.5 Safari/605.1.15`) against Chromium 152.0.7977.65 in
Electron 44.1.1.

| Feature the front uses                               | WebKitGTK | Chromium |
| ---------------------------------------------------- | --------- | -------- |
| `oklch()`                                            | yes       | yes      |
| `container-type: inline-size`                        | yes       | yes      |
| `color-mix(in oklch, ...)`                           | yes       | yes      |
| `CSS.registerProperty` with a `<color>` registration | yes       | yes      |
| `backdrop-filter: blur()`                            | yes       | yes      |
| `:focus-visible`                                     | yes       | yes      |
| `inset`                                              | yes       | yes      |

Computed colours on the rendered Home page:

| Element             | Property                     | WebKitGTK                                 | Chromium                                     |
| ------------------- | ---------------------------- | ----------------------------------------- | -------------------------------------------- |
| Play button         | background                   | `rgb(126, 80, 30)`                        | `rgb(126, 80, 30)`                           |
| Play button         | border                       | `rgb(212, 151, 84)`                       | `rgb(212, 151, 84)`                          |
| Sidebar active item | left border                  | `rgb(126, 80, 30)`                        | `rgb(126, 80, 30)`                           |
| Sidebar active item | background                   | `oklab(0.47366 0.038302 0.080373 / 0.15)` | `oklab(0.473662 0.0383235 0.0803802 / 0.15)` |
| `text-vsl`          | color                        | `rgb(212, 151, 84)`                       | `rgb(212, 151, 84)`                          |
| `:root`             | `--color-vs` / `--color-vsl` | `#7e501e` / `#d49754`                     | `#7e501e` / `#d49754`                        |

Every value matches. The single difference is serialisation precision in the `oklab()` form
both engines print for `bg-vs/15`, five significant figures against six, same colour. Both
keep the value in `oklab` rather than resolving it to `rgb`, so the Tailwind v4 accent ramp
survives the move unchanged.

Navigating `#/config`, then `#/installations`, then back to `#/` renders correctly on both,
with zero console errors on Tauri. Electron logged one warning, present in the shipped
build and unrelated to the port (`frame-ancestors` ignored when delivered via a `<meta>`
element). Rendered body text is slightly shorter on Tauri per route (726 characters against
818 on `#/config`), which is the stubs refusing data, not a rendering difference.

No compatibility problem was found.

## Risks observed

**The WebKitGTK version a user's distribution ships.** Only 2.52.6 was tested, and it is
clean. The floor is set by the front end, not by Tauri: Tailwind v4's documented baseline is
Safari 16.4, which is the feature set WebKitGTK 2.40 carries (March 2023), and
`color-mix()` plus `CSS.registerProperty` are the two probes above that need it. Anything
older paints the accent ramp wrong rather than failing loudly, which is the bad kind of
failure. The deb and rpm hand that variable to the user's distribution; the AppImage does
not, because it carries its own copy. Which versions the distributions the team's players
actually run are shipping was not checked, and that check is the one that decides whether
the small packages are usable.

**The updater.** `appUpdater` is six stubs and the whole electron-updater arrangement is
gone with them. `tauri-plugin-updater` is a different model: signed release manifests with
a key pair the project has to hold, and no equivalent of the beta channel switch
`receiveBetaUpdates` currently drives. Nothing about it was evaluated here. For a launcher
that updates itself in front of players, this is the largest unexamined piece.

**No pacman target.** Tauri's Linux bundler produces AppImage, deb and rpm. The Electron
build also ships a pacman package (87.7 MiB), and losing it means Arch and Manjaro users
move to the AppImage or to a PKGBUILD somebody maintains.

**No Windows bundle.** Deliberately out of the skeleton PR: it needs its own WiX or NSIS
choice and an upgrade-path decision against the installers already in players' hands. The
Windows CI leg compiles and tests the host, so the code is known to build there; nothing is
known about the installer.

**The app has never been run outside the measurement harness, and the capability set is
unconfirmed.** `src-tauri/capabilities/default.json` lists `core:event`, `core:window` and
`core:webview` and leaves out the tray, menu, image and resource handles and the path API,
on the reasoning that application commands are not gated by the access control list. The
spike build did come up and carry an `invoke`, which is evidence that this is enough, but
the spike binary carried two temporary additions. Widening it is one line if a normal run
disagrees.

**No credential store equivalent.** Not a risk found by measuring, but it falls out of the
port surface. `src/ipc/accountStore.ts` (361 lines) is built on Electron's `safeStorage`,
which refuses to run on Linux without a system password store. Tauri core has no
counterpart, and `tauri-plugin-stronghold` is a password-derived vault rather than an OS
keyring, so the accounts feature needs a design decision and not just a translation.

## What a full port would still need

The 46 stubbed members, by group, with the Electron handler each one would replace. The
ported three took about 1.5 lines of Rust per line of the TypeScript handler they replace,
so the last column applies that ratio; treat it as an order of magnitude, not an estimate.

| Group                | Stubs | Electron source                                                         | TS lines            | Rough Rust | What it drags in                                                                                                                                                                                        |
| -------------------- | ----- | ----------------------------------------------------------------------- | ------------------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pathsManager`       | 19    | `pathsHandlers.ts`, `installerTimeoutOutcome.ts`                        | 844                 | ~1300      | download with progress, zip extract and compress with progress, the Windows installer path, `tauri-plugin-dialog`, `tauri-plugin-opener`, and the `pathPolicy.ts` gate (249 lines) underneath all of it |
| `accountManager`     | 2     | `accountHandlers.ts` and its three outcome files, `accountStore.ts`     | 837                 | ~1250      | the credential store decision above, plus the login wire format and its failure taxonomy                                                                                                                |
| `gameManager`        | 1     | the rest of `gameHandlers.ts`                                           | ~530 ported in part | ~200       | listing a version folder, which is also the fix for the restated launch plan                                                                                                                            |
| `modsManager`        | 8     | `modsHandlers.ts`, `archiveValidation.ts`                               | 498                 | ~750       | reading `modinfo.json` out of archives, the icon cache, modpack import and export                                                                                                                       |
| `netManager`         | 3     | `netHandlers.ts`, `network.ts`                                          | 567                 | ~850       | an HTTP client with byte caps, timeouts and GitHub rate limit handling                                                                                                                                  |
| `utils`              | 5     | `utilsHandlers.ts`                                                      | 94                  | ~150       | the folder dialog and the browser opener are plugins; the close guard is window plumbing                                                                                                                |
| `appUpdater`         | 6     | `appUpdaterHandlers.ts`, plus the electron-updater wiring in `src/main` | 88 plus             | unknown    | `tauri-plugin-updater`, a signing key pair, a release manifest and the beta channel question                                                                                                            |
| `backgroundsManager` | 2     | `backgroundHandlers.ts`                                                 | 118                 | ~180       | hash-checked downloads into the cache, JPEG sniffing                                                                                                                                                    |

Roughly 3 000 lines of TypeScript host code still to translate, plus `src/ipc/validation.ts`
(473 lines) and `pathPolicy.ts` (249) which most of the handlers above sit on, and whatever
`src/main` (995 lines across six files) holds for the window and the updater. The 87 test
files deleted with the Electron host were all tests of modules that no longer exist; their
Rust replacements are part of that figure and not counted in it. Nothing under
`tests/domain`, `tests/renderer`, `tests/renderer-dom` or `tests/i18n` was touched.

## Reproducing

The measurement artifacts (`measurements.json` with every run and the full compatibility
snapshots, the raw timing records, the per run process trees, the harness, the profile
seeder, the probes) were kept outside the repository; ask for them if a re-run needs the
exact scripts.

The binary measured here carried two temporary additions that are not in this repository: a
`spike_record` command appending a JSON line to the file named by `RIFTLAUNCHER_SPIKE_LOG`,
and an `on_page_load` hook evaluating the script named by `RIFTLAUNCHER_SPIKE_SCRIPT`. Both
are inert without those variables. It was built with `tauri build --no-bundle`, since a
plain `cargo build --release` points the binary at the dev URL. The working tree was
restored afterwards and the pristine release binary copied back, byte identical; the
shipped bundles were never overwritten.
