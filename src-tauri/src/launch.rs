//! Starting one installed version of Vintage Story.
//!
//! What the plan below decides is what `src/domain/versions/launch.ts` decides,
//! rule for rule, and the parity is not cosmetic: the argument shape, the mono
//! fallback and the Mesa variable are all things a player's game already depends
//! on. The rules are restated here rather than called into because the front
//! cannot list a directory, and picking the executable means reading the version
//! folder. The tests at the bottom of this file pin each of the three details the
//! TypeScript pins, so the two cannot drift quietly.
//!
//! Nothing here trusts the request. The renderer names a version and an
//! installation by id; everything else is read out of the saved config, and the
//! two folders have to sit inside a root that config names before anything is
//! spawned.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use serde::Serialize;

/// The variable the launcher sets for the "Mesa GL thread" checkbox.
///
/// Lowercase, because that is the name Mesa's driconf override actually reads
/// from the environment. The uppercase spelling the launcher inherited set a
/// variable Mesa never looked at.
const MESA_GL_THREAD_VARIABLE: &str = "mesa_glthread";

/// The fixed line the .NET host prints when no installed runtime satisfies the
/// framework a build asks for. The host's own wording, so it is the same on
/// every distribution and for every game version.
const MISSING_DOTNET_SENTINEL: &str = "You must install or update .NET to run this application.";

/// How much of stderr is kept for the sentinel scan. The host prints the sentence
/// first, so a few kilobytes is far more than enough, and the bound is what keeps
/// a chatty game from growing a string for the length of a play session.
const STDERR_SCAN_LIMIT: usize = 4_096;

/// How much of one play session's output reaches the log, per stream.
///
/// ponytail: a flat per-session cap, not a rotating log. A game that prints for
/// six hours would otherwise fill the disk. Swap in a rolling file if anyone
/// ever needs the tail of a long session rather than its head.
const LOG_CAPTURE_LIMIT: usize = 1024 * 1024;

/// EXECUTE_GAME's verdict, in the shape `GameExecutionResult` declares on the front.
#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum GameExecutionResult {
    Ran {
        ok: bool,
        #[serde(rename = "exitCode")]
        exit_code: Option<i32>,
    },
    Refused {
        ok: bool,
        reason: &'static str,
    },
}

impl GameExecutionResult {
    pub fn ran(exit_code: Option<i32>) -> Self {
        Self::Ran {
            ok: true,
            exit_code,
        }
    }

    /// One of `GameExecutionFailureReason`. Anything else would reach the renderer
    /// as a refusal it has no sentence for.
    pub fn refused(reason: &'static str) -> Self {
        Self::Refused { ok: false, reason }
    }
}

/// Everything the host needs to start the game, and nothing it has to decide.
#[derive(Debug, PartialEq, Eq)]
pub struct LaunchPlan {
    /// What is spawned: the game, `mono`, or the wrapper in front of either.
    pub command: String,
    pub args: Vec<String>,
    /// The game file itself, which is what gets checked before the spawn.
    pub executable_path: PathBuf,
    /// Variables this platform adds, on top of whatever the process already has.
    pub env: BTreeMap<String, String>,
    /// Folder the game runs in, which is the version folder.
    pub cwd: PathBuf,
}

/// Why no command was built. The same two words the TypeScript uses, because they
/// go onto the wire unchanged.
#[derive(Debug, PartialEq, Eq)]
pub enum PlanFailure {
    UnsupportedPlatform,
    NoExecutable,
}

impl PlanFailure {
    fn reason(&self) -> &'static str {
        match self {
            Self::UnsupportedPlatform => "unsupported-platform",
            Self::NoExecutable => "no-executable",
        }
    }
}

/// The host platform, spelled the way the domain spells it, or `None` for one the
/// launcher cannot start the game on. macOS is left out for the reason it always
/// has been: no macOS launch has ever been implemented.
fn launchable_os() -> Option<&'static str> {
    match std::env::consts::OS {
        "linux" => Some("linux"),
        "windows" => Some("win32"),
        _ => None,
    }
}

/// File names that prove the game landed in a folder, paired with how each is
/// started, in the order the launcher looks for them.
///
/// Windows only ever ships `Vintagestory.exe`. Linux ships the native
/// `Vintagestory` launcher and, on older builds, the same `Vintagestory.exe`
/// Windows ships, which Linux can only run through `mono`.
fn executable_candidates(os: &str) -> &'static [(&'static str, bool)] {
    match os {
        "win32" => &[("Vintagestory.exe", false)],
        "linux" => &[("Vintagestory", false), ("Vintagestory.exe", true)],
        _ => &[],
    }
}

/// Works out how to start one installed version.
///
/// Three details are inherited rather than chosen, and each is pinned by a test
/// below because changing it changes what a player's game does:
///
/// - The start parameters go in as ONE argument, whatever they contain, and are
///   always passed, so an installation with none gives the game one empty argument.
/// - Under `mono` the executable path leads the argument list, ahead of `--dataPath`.
/// - The Mesa variable is set for the native Linux launcher only. The mono
///   fallback never got it, and neither did Windows.
#[allow(clippy::too_many_arguments)]
pub fn build_plan(
    os: &str,
    version_folder: &Path,
    file_names: &[String],
    installation_path: &str,
    start_params: &str,
    mesa_gl_thread: bool,
    launch_wrapper: &str,
) -> Result<LaunchPlan, PlanFailure> {
    let candidates = executable_candidates(os);
    if candidates.is_empty() {
        return Err(PlanFailure::UnsupportedPlatform);
    }

    let (file_name, runs_under_mono) = candidates
        .iter()
        .find(|(name, _)| file_names.iter().any(|entry| entry == name))
        .ok_or(PlanFailure::NoExecutable)?;

    let executable_path = version_folder.join(file_name);
    let executable = executable_path.to_string_lossy().into_owned();

    let game_args = vec![
        format!("--dataPath={installation_path}"),
        start_params.to_string(),
    ];
    let game_command = if *runs_under_mono {
        "mono".to_string()
    } else {
        executable.clone()
    };
    let game_command_args = if *runs_under_mono {
        let mut args = vec![executable];
        args.extend(game_args);
        args
    } else {
        game_args
    };

    let wrapper = launch_wrapper.trim();
    let wrapped = !wrapper.is_empty() && os == "linux";

    let mut env = BTreeMap::new();
    if !runs_under_mono && os == "linux" && mesa_gl_thread {
        env.insert(MESA_GL_THREAD_VARIABLE.to_string(), "true".to_string());
    }

    Ok(LaunchPlan {
        command: if wrapped {
            wrapper.to_string()
        } else {
            game_command.clone()
        },
        args: if wrapped {
            let mut args = vec![game_command];
            args.extend(game_command_args);
            args
        } else {
            game_command_args
        },
        executable_path,
        env,
        cwd: version_folder.to_path_buf(),
    })
}

/// Variable names a player's installation may never set.
///
/// Every one of these changes what code a process loads rather than how it
/// behaves, so an installation that could set them could make the launcher start
/// something other than the game it named.
const DENIED_ENVIRONMENT_KEYS: &[&str] = &[
    "PATH",
    "PATHEXT",
    "LD_PRELOAD",
    "LD_AUDIT",
    "LD_DEBUG",
    "LD_ASSUME_KERNEL",
    "LD_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "GCONV_PATH",
    "BASH_ENV",
    "ENV",
    "PERL5OPT",
    "PERL5LIB",
    "PYTHONPATH",
    "PYTHONINSPECT",
    "RUBYOPT",
    "RUBYLIB",
    "NODE_OPTIONS",
    "NODE_PATH",
    "ELECTRON_RUN_AS_NODE",
    "ELECTRON_NO_ATTACH_CONSOLE",
    "JAVA_TOOL_OPTIONS",
    "_JAVA_OPTIONS",
    "CLASSPATH",
    "DOTNET_STARTUP_HOOKS",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_SSH_COMMAND",
];

const DENIED_ENVIRONMENT_PREFIXES: &[&str] = &["COMPLUS_", "DYLD_", "LD_"];

/// Reads an installation's `envVars` field, refusing the whole string if any one
/// entry is not a plain variable assignment the launcher is willing to make.
///
/// Refusing the whole string rather than dropping the bad entry is deliberate: a
/// player who typed something the launcher will not set should be told, not
/// quietly given a different environment than the one they wrote.
pub fn parse_safe_environment(value: &str) -> Result<BTreeMap<String, String>, ()> {
    let mut environment = BTreeMap::new();
    if value.is_empty() {
        return Ok(environment);
    }

    let entries: Vec<&str> = value.split(',').collect();
    if entries.len() > 128 {
        return Err(());
    }

    for entry in entries {
        let separator = entry.find('=').ok_or(())?;
        if separator == 0 {
            return Err(());
        }
        let key = entry[..separator].trim();
        let entry_value = &entry[separator + 1..];
        let normalized = key.to_uppercase();

        let well_formed = key
            .chars()
            .next()
            .map(|first| first.is_ascii_alphabetic() || first == '_')
            .unwrap_or(false)
            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');

        if !well_formed
            || DENIED_ENVIRONMENT_KEYS.contains(&normalized.as_str())
            || DENIED_ENVIRONMENT_PREFIXES
                .iter()
                .any(|prefix| normalized.starts_with(prefix))
            || entry_value.len() > 2_048
            || entry_value.contains('\0')
        {
            return Err(());
        }

        environment.insert(key.to_string(), entry_value.to_string());
    }

    Ok(environment)
}

/// True when `path` is a regular file the current user may execute.
fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Resolves a player-chosen Linux wrapper to an absolute executable, with no shell.
///
/// A bare name is looked up in the launcher's own PATH, an absolute path is taken
/// as given, and anything relative is refused: a relative command resolves against
/// the spawned process's working directory, which is the game version folder
/// rather than wherever the player was thinking of. PATH entries that are
/// themselves relative are skipped for the same reason, so what is checked here
/// and what gets spawned cannot be two different files.
///
/// What this cannot give the launcher is a hold on the game once a wrapper hands
/// it off. A wrapper that execs the game keeps the launcher's pipes and so keeps
/// it waiting for the real session; one that detaches properly ends the session
/// early, and the playtime recorded is the wrapper's lifetime.
pub fn resolve_launch_wrapper(value: &str) -> Option<PathBuf> {
    let wrapper = value.trim();
    if wrapper.is_empty() {
        return None;
    }

    let candidate = Path::new(wrapper);
    if candidate.is_absolute() {
        return is_executable_file(candidate).then(|| candidate.to_path_buf());
    }
    if wrapper.contains('/') || wrapper.contains('\\') {
        return None;
    }

    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join(wrapper))
        .find(|candidate| is_executable_file(candidate))
}

/// True when the executable is a regular file and not a symbolic link.
///
/// A game executable that is a link means the version folder is not what the
/// launcher installed. Wrappers are checked with `is_executable_file` instead,
/// which follows links, because `/usr/bin/gamemoderun` and friends are symbolic
/// links on most distributions and refusing them would refuse the feature.
fn is_real_executable(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(metadata) => metadata.is_file() && !metadata.file_type().is_symlink(),
        Err(_) => false,
    }
}

/// Drains one pipe into the log, and returns the head of what it saw.
fn drain(
    mut stream: impl Read + Send + 'static,
    label: &'static str,
    log: Arc<Mutex<fs::File>>,
) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut head = String::new();
        let mut written = 0usize;
        let mut buffer = [0u8; 8192];

        loop {
            match stream.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    let chunk = String::from_utf8_lossy(&buffer[..read]);
                    if head.len() < STDERR_SCAN_LIMIT {
                        let room = STDERR_SCAN_LIMIT - head.len();
                        head.push_str(&chunk[..chunk.len().min(room)]);
                    }
                    if written < LOG_CAPTURE_LIMIT {
                        let room = LOG_CAPTURE_LIMIT - written;
                        let slice = &chunk[..chunk.len().min(room)];
                        written += slice.len();
                        if let Ok(mut file) = log.lock() {
                            let _ = write!(file, "[{label}] {slice}");
                        }
                    }
                }
            }
        }

        head
    })
}

/// Runs the game and returns when the player closes it.
///
/// The exit code is reported without being judged: Vintage Story exits non-zero
/// often enough that reading that as a failed launch would tell a player their
/// session went wrong after they closed it themselves. The one thing the output
/// is read for is the .NET host's missing-runtime sentence, which is what a
/// player gets instead of a game when the build needs a runtime they do not have.
fn run(
    plan: &LaunchPlan,
    environment: BTreeMap<String, String>,
    log_path: &Path,
) -> GameExecutionResult {
    if let Some(directory) = log_path.parent() {
        let _ = fs::create_dir_all(directory);
    }

    let log = match fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        Ok(file) => Arc::new(Mutex::new(file)),
        Err(err) => {
            eprintln!("[host] [launch] could not open the game log: {err}");
            return GameExecutionResult::refused("launch-failed");
        }
    };

    if let Ok(mut file) = log.lock() {
        let _ = writeln!(
            file,
            "\n[host] running {} in {}",
            plan.command,
            plan.cwd.display()
        );
    }

    let mut command = Command::new(&plan.command);
    command
        .args(&plan.args)
        .current_dir(&plan.cwd)
        .env_clear()
        .envs(
            environment
                .iter()
                .map(|(k, v)| (OsString::from(k), OsString::from(v))),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            eprintln!("[host] [launch] could not start the game: {err}");
            return GameExecutionResult::refused("launch-failed");
        }
    };

    let stdout = child
        .stdout
        .take()
        .map(|pipe| drain(pipe, "out", Arc::clone(&log)));
    let stderr = child
        .stderr
        .take()
        .map(|pipe| drain(pipe, "err", Arc::clone(&log)));

    let status = child.wait();
    if let Some(handle) = stdout {
        let _ = handle.join();
    }
    let stderr_head = stderr
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();

    match status {
        Ok(status) => {
            if stderr_head.contains(MISSING_DOTNET_SENTINEL) {
                GameExecutionResult::refused("missing-dotnet")
            } else {
                GameExecutionResult::ran(status.code())
            }
        }
        Err(err) => {
            eprintln!("[host] [launch] the game process could not be waited on: {err}");
            GameExecutionResult::refused("launch-failed")
        }
    }
}

/// What `launch_game` needs out of the config once the ids have been looked up.
pub struct LaunchRequest {
    pub version_folder: PathBuf,
    pub installation_path: String,
    pub start_params: String,
    pub mesa_gl_thread: bool,
    pub launch_wrapper: String,
    pub env_vars: String,
}

/// Starts the game, or says why it did not.
pub fn launch(request: LaunchRequest, log_path: &Path) -> GameExecutionResult {
    let Some(os) = launchable_os() else {
        return GameExecutionResult::refused("unsupported-platform");
    };

    let Ok(environment) = parse_safe_environment(&request.env_vars) else {
        eprintln!("[host] [launch] refused this installation's environment variables");
        return GameExecutionResult::refused("invalid-request");
    };

    let mut wrapper = String::new();
    if os == "linux" && !request.launch_wrapper.trim().is_empty() {
        match resolve_launch_wrapper(&request.launch_wrapper) {
            Some(resolved) => wrapper = resolved.to_string_lossy().into_owned(),
            None => {
                eprintln!(
                    "[host] [launch] refused an unavailable or non-executable launch wrapper"
                );
                return GameExecutionResult::refused("launch-failed");
            }
        }
    }

    let file_names = match fs::read_dir(&request.version_folder) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        Err(err) => {
            eprintln!("[host] [launch] could not read the version folder: {err}");
            return GameExecutionResult::refused("no-executable");
        }
    };

    let plan = match build_plan(
        os,
        &request.version_folder,
        &file_names,
        &request.installation_path,
        &request.start_params,
        request.mesa_gl_thread,
        &wrapper,
    ) {
        Ok(plan) => plan,
        Err(failure) => return GameExecutionResult::refused(failure.reason()),
    };

    if !is_real_executable(&plan.executable_path) {
        eprintln!("[host] [launch] refused to run an invalid game executable");
        return GameExecutionResult::refused("launch-failed");
    }

    // The launcher's own environment first, then the installation's, then what the
    // plan adds. Same order the Electron host spread them in.
    let mut merged: BTreeMap<String, String> = std::env::vars().collect();
    merged.extend(environment);
    merged.extend(plan.env.clone());

    run(&plan, merged, log_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(entries: &[&str]) -> Vec<String> {
        entries.iter().map(|entry| entry.to_string()).collect()
    }

    /// The game file, spelled the way this platform spells it.
    ///
    /// `build_plan` reaches it with `Path::join`, so an expectation written with a
    /// forward slash only matches on Unix. Joining here too keeps these tests about
    /// the launch rules rather than about the separator the host happens to use.
    fn game(folder: &str, name: &str) -> String {
        Path::new(folder).join(name).to_string_lossy().into_owned()
    }

    #[test]
    fn the_native_linux_launcher_runs_directly_with_the_data_path_and_the_start_parameters() {
        let plan = build_plan(
            "linux",
            Path::new("/versions/1.20.0"),
            &names(&["Vintagestory", "assets"]),
            "/installs/survival",
            "--openWorld",
            false,
            "",
        )
        .expect("a plan");

        assert_eq!(plan.command, game("/versions/1.20.0", "Vintagestory"));
        assert_eq!(
            plan.args,
            vec!["--dataPath=/installs/survival", "--openWorld"]
        );
        assert_eq!(plan.cwd, Path::new("/versions/1.20.0"));
        assert!(plan.env.is_empty());
    }

    #[test]
    fn empty_start_parameters_still_reach_the_game_as_one_argument() {
        let plan = build_plan(
            "linux",
            Path::new("/v"),
            &names(&["Vintagestory"]),
            "/i",
            "",
            false,
            "",
        )
        .expect("a plan");
        assert_eq!(plan.args, vec!["--dataPath=/i", ""]);
    }

    #[test]
    fn start_parameters_are_never_split_on_spaces() {
        let plan = build_plan(
            "linux",
            Path::new("/v"),
            &names(&["Vintagestory"]),
            "/i",
            "--foo --bar",
            false,
            "",
        )
        .expect("a plan");
        assert_eq!(plan.args.len(), 2);
        assert_eq!(plan.args[1], "--foo --bar");
    }

    #[test]
    fn the_exe_fallback_runs_under_mono_with_the_executable_leading_the_arguments() {
        let plan = build_plan(
            "linux",
            Path::new("/v"),
            &names(&["Vintagestory.exe"]),
            "/i",
            "",
            false,
            "",
        )
        .expect("a plan");

        assert_eq!(plan.command, "mono");
        assert_eq!(
            plan.args,
            vec![
                game("/v", "Vintagestory.exe"),
                "--dataPath=/i".to_string(),
                String::new()
            ]
        );
        assert_eq!(
            plan.executable_path,
            Path::new("/v").join("Vintagestory.exe")
        );
    }

    #[test]
    fn the_mesa_variable_is_set_for_the_native_linux_launcher_only() {
        let native = build_plan(
            "linux",
            Path::new("/v"),
            &names(&["Vintagestory"]),
            "/i",
            "",
            true,
            "",
        )
        .expect("a plan");
        assert_eq!(
            native.env.get(MESA_GL_THREAD_VARIABLE).map(String::as_str),
            Some("true")
        );

        let mono = build_plan(
            "linux",
            Path::new("/v"),
            &names(&["Vintagestory.exe"]),
            "/i",
            "",
            true,
            "",
        )
        .expect("a plan");
        assert!(mono.env.is_empty());

        let windows = build_plan(
            "win32",
            Path::new("/v"),
            &names(&["Vintagestory.exe"]),
            "/i",
            "",
            true,
            "",
        )
        .expect("a plan");
        assert!(windows.env.is_empty());
    }

    #[test]
    fn a_wrapper_takes_the_command_and_pushes_the_game_into_the_arguments() {
        let plan = build_plan(
            "linux",
            Path::new("/v"),
            &names(&["Vintagestory"]),
            "/i",
            "",
            false,
            "/usr/bin/gamemoderun",
        )
        .expect("a plan");

        assert_eq!(plan.command, "/usr/bin/gamemoderun");
        assert_eq!(
            plan.args,
            vec![
                game("/v", "Vintagestory"),
                "--dataPath=/i".to_string(),
                String::new()
            ]
        );
    }

    #[test]
    fn a_wrapper_is_ignored_off_linux() {
        let plan = build_plan(
            "win32",
            Path::new("/v"),
            &names(&["Vintagestory.exe"]),
            "/i",
            "",
            false,
            "C:\\wrap.exe",
        )
        .expect("a plan");
        assert_eq!(plan.command, game("/v", "Vintagestory.exe"));
    }

    #[test]
    fn a_folder_with_no_game_in_it_has_no_plan() {
        assert_eq!(
            build_plan(
                "linux",
                Path::new("/v"),
                &names(&["readme.txt"]),
                "/i",
                "",
                false,
                ""
            ),
            Err(PlanFailure::NoExecutable)
        );
    }

    #[test]
    fn a_platform_the_launcher_cannot_start_the_game_on_has_no_plan() {
        assert_eq!(
            build_plan(
                "darwin",
                Path::new("/v"),
                &names(&["Vintagestory"]),
                "/i",
                "",
                false,
                ""
            ),
            Err(PlanFailure::UnsupportedPlatform)
        );
    }

    #[test]
    fn plain_variables_are_accepted_and_loader_variables_are_not() {
        let parsed =
            parse_safe_environment("LANG=en_US,VS_LAUNCHER_TEST=1").expect("an environment");
        assert_eq!(parsed.get("LANG").map(String::as_str), Some("en_US"));
        assert_eq!(
            parsed.get("VS_LAUNCHER_TEST").map(String::as_str),
            Some("1")
        );

        assert!(parse_safe_environment("PATH=/tmp").is_err());
        assert!(parse_safe_environment("LD_PRELOAD=/tmp/x.so").is_err());
        assert!(parse_safe_environment("DYLD_ANYTHING=1").is_err());
        assert!(parse_safe_environment("NODE_OPTIONS=--require=/tmp/payload").is_err());
        assert!(parse_safe_environment("bad-entry").is_err());
        assert!(parse_safe_environment("=value").is_err());
    }

    #[test]
    fn an_empty_environment_string_is_an_empty_environment() {
        assert!(parse_safe_environment("")
            .expect("an environment")
            .is_empty());
    }

    #[test]
    fn a_relative_wrapper_is_refused_rather_than_resolved_against_the_game_folder() {
        assert_eq!(resolve_launch_wrapper("./wrapper"), None);
        assert_eq!(resolve_launch_wrapper("tools/wrapper"), None);
        assert_eq!(resolve_launch_wrapper("   "), None);
    }

    #[test]
    fn a_refusal_carries_a_reason_the_renderer_knows() {
        let refused =
            serde_json::to_value(GameExecutionResult::refused("no-executable")).expect("json");
        assert_eq!(refused["ok"], serde_json::json!(false));
        assert_eq!(refused["reason"], serde_json::json!("no-executable"));

        let ran = serde_json::to_value(GameExecutionResult::ran(Some(0))).expect("json");
        assert_eq!(ran["ok"], serde_json::json!(true));
        assert_eq!(ran["exitCode"], serde_json::json!(0));
    }
}
