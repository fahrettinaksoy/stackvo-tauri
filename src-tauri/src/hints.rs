//! Every suggestion this app makes to a user, in one place and translatable.
//!
//! ## The problem this closes
//!
//! `ErrorAlert.vue` shows a translated category heading over the specific
//! message, which is the right design and was already right. But underneath it
//! printed the `hint` **raw**, and every hint in the codebase was an English
//! literal written at the point it was raised — 57 of them, scattered across 25
//! modules. So a Turkish user saw a translated heading, an English explanation,
//! and an English suggestion.
//!
//! The suggestion is the worst one to leave untranslated. It is the sentence
//! that tells someone what to *do*: start Docker, choose a folder, adopt the
//! directory instead. A message they cannot read is a failure they cannot act
//! on.
//!
//! ## Why a catalogue rather than a key at each call site
//!
//! Passing a bare key — `.with_hint_key("startDocker")` — would have worked and
//! would have left the English text in 25 files and the key in 25 more places to
//! typo. A `Hint` is both halves at once, declared once, referenced by name:
//!
//! ```ignore
//! Err(Error::new(Code::EngineUnreachable, "...").with_hint(hints::START_DOCKER))
//! ```
//!
//! The call site reads better than the string it replaced, the compiler catches
//! a wrong name, and — the reason the readiness review wanted this — the whole
//! set is now **reviewable in one file** instead of being a grep across the
//! codebase.
//!
//! ## English is still carried
//!
//! Each `Hint` keeps its English text, and `Error::with_hint` still fills the
//! `hint` field with it. That is what the log records, what an MCP client sees,
//! and what the UI falls back to if a locale is missing the key. Translation is
//! an addition to the existing behaviour, never a replacement for it.
//!
//! `tests/hint_translations.rs` is what keeps that promise honest: it fails if a
//! hint here has no entry in `en.js`, or no entry in `tr.js`, or if a locale
//! carries a key nothing raises any more.

/// A suggestion, with the key a locale file translates it under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hint {
    /// Looked up as `errorHints.<key>` in the locale files.
    pub key: &'static str,
    /// The fallback, and what the log and the MCP surface get.
    pub english: &'static str,
}

/// Declare a hint and enrol it in [`ALL`] in one go.
///
/// The enrolment is the point: a hint added without it would be invisible to
/// the translation test, which is the only thing standing between "we translate
/// hints" and "we translate the hints somebody remembered".
macro_rules! hints {
    ($($(#[$doc:meta])* $name:ident = $key:literal, $english:literal;)*) => {
        $($(#[$doc])* pub const $name: Hint = Hint { key: $key, english: $english };)*

        /// Every hint in this file, for the translation test.
        pub const ALL: &[Hint] = &[$($name),*];
    };
}

hints! {
    // ---------------------------------------------------------------- engine
    START_DOCKER = "startDocker",
        "Start Docker Desktop and try again.";
    START_DOCKER_OR_SET_HOST = "startDockerOrSetHost",
        "Start Docker Desktop, or set DOCKER_HOST if the engine is elsewhere.";
    START_DOCKER_MANUALLY = "startDockerManually",
        "Start Docker manually, then retry.";
    PROJECT_MAY_NOT_BE_BUILT = "projectMayNotBeBuilt",
        "The project may not be built yet.";

    // ---------------------------------------------------------------- workspace
    CHOOSE_WORKSPACE = "chooseWorkspace",
        "Choose an empty folder for StackVo to set up, or one it already manages.";
    PROJECT_NAME_CHARSET = "projectNameCharset",
        "Names may contain letters, digits, dot, underscore and dash, and must start with a letter or digit.";
    PATH_LEAVES_PROJECTS = "pathLeavesProjects",
        "Refusing to operate on a path that leaves projects/.";
    ONLY_PROJECT_FOLDERS = "onlyProjectFolders",
        "Only project folders inside the selected workspace can be opened.";

    // ---------------------------------------------------------------- projects
    ADOPT_INSTEAD = "adoptInstead",
        "Adopt it instead — that is the path that writes one.";
    FIX_OR_ADOPT = "fixOrAdopt",
        "Fix the file, or delete it and adopt the folder instead.";
    RUN_DOCTOR_THEN_RETRY = "runDoctorThenRetry",
        "Settings → Doctor lists what is wrong and can repair it; then clone or register again.";
    ADOPT_EXISTING_CODE = "adoptExistingCode",
        "Use adoption for existing code — scaffolding is for a brand-new project.";
    CHOOSE_ANOTHER_NAME = "chooseAnotherName",
        "Choose another name, or adopt the folder that is already there.";
    INSTALL_GIT_OR_ADOPT = "installGitOrAdopt",
        "Install git, or clone the repository yourself and adopt the folder.";
    EDIT_FROM_MANIFEST_TAB = "editFromManifestTab",
        "Edit it from the project's Manifest tab instead.";
    START_PROJECT_FOR_COMMANDS = "startProjectForCommands",
        "Start the project first — these commands run inside its container.";
    BUILD_AND_START_FOR_WORKER = "buildAndStartForWorker",
        "Build and start the project first — the worker runs its image.";
    WORKERS_ARE_DETECTED = "workersAreDetected",
        "Workers are detected from artisan and composer.json.";
    START_PROJECT_FOR_TUNNEL = "startProjectForTunnel",
        "Start the project first — the tunnel forwards to its container.";

    // ---------------------------------------------------------------- certificates
    INSTALL_MKCERT = "installMkcert",
        "Install it with `brew install mkcert` (macOS), your package manager (Linux), \
         or `choco install mkcert` (Windows), then try again.";
    CHECK_TLD_AND_DOMAINS = "checkTldAndDomains",
        "Check DEFAULT_TLD_SUFFIX in .env and the `domain` in each stackvo.json.";
    CERTIFICATE_ISSUED_BUT_UNTRUSTED = "certificateIssuedButUntrusted",
        "The certificate is issued either way and the stack serves — the browser warns \
         about the issuer until the authority is trusted. Settings → Certificates has \
         a button that does it in your terminal, where the password prompt can be answered.";
    RUN_MKCERT_INSTALL = "runMkcertInstall",
        "Run `mkcert -install` once in a terminal — it needs a password for the \
         system trust store, and a windowed app has no terminal to ask in.";

    // ---------------------------------------------------------------- hosts file
    HOSTNAME_CHARSET = "hostnameCharset",
        "Hostnames may contain letters, digits, dots and hyphens.";
    HOSTS_NEEDS_ADMIN = "hostsNeedsAdmin",
        "Administrator rights are required to edit the hosts file.";
    HOSTS_NOT_REPLACED = "hostsNotReplaced",
        "The hosts file could not be replaced.";
    INSTALL_POLKIT = "installPolkit",
        "Install polkit, or edit /etc/hosts manually.";

    // ---------------------------------------------------------------- services
    SERVICE_MUST_BE_IN_CATALOG = "serviceMustBeInCatalog",
        "Only services listed in contracts/env.schema.json can be managed.";
    SUPPORTED_DATABASES = "supportedDatabases",
        "Supported: mysql, mariadb, postgres, mongo.";
    ENABLE_A_MAIL_CATCHER = "enableAMailCatcher",
        "Enable mailhog (or mailpit) in .env, then regenerate.";
    MAIL_UI_MAY_BE_STARTING = "mailUiMayBeStarting",
        "The container may still be starting, or its UI port may be taken.";

    // ---------------------------------------------------------------- configuration
    ENV_KEY_CHARSET = "envKeyCharset",
        "Keys must match ^[A-Z_][A-Z0-9_]*$ so Compose can interpolate them.";
    ENV_IS_ONE_KEY_PER_LINE = "envIsOneKeyPerLine",
        "The .env format is one key per line; multi-line values cannot be read back.";
    REVEAL_VALUE_FIRST = "revealValueFirst",
        "Reveal the value first, or leave the field untouched.";
    PHP_INI_DIRECTIVE_CHARSET = "phpIniDirectiveCharset",
        "Directive names are letters, digits, underscores and dots.";
    PHP_INI_IS_ONE_PER_LINE = "phpIniIsOnePerLine",
        "php.ini is one directive per line.";
    PHP_INI_SIZE_FORMAT = "phpIniSizeFormat",
        "Sizes are a number with an optional K, M or G — 256M, 1G, 512. \
         Times are whole seconds. -1 means unlimited.";
    SERVER_DIRECTIVES_UNSUPPORTED = "serverDirectivesUnsupported",
        "Only nginx, caddy and frankenphp have a generated config to add directives to.";
    SETTING_IS_MANAGED = "settingIsManaged",
        "This value comes from a policy file on this machine. Ask whoever administers it.";
    UNLOCK_THE_KEYSTORE = "unlockTheKeystore",
        "Unlock your keychain and try again — the password for this setting is stored there.";
    ONLY_CREDENTIALS_MOVE = "onlyCredentialsMove",
        "Only passwords, tokens and server ids can be kept in the keystore.";
    KEYSTORE_ENTRY_IS_GONE = "keystoreEntryIsGone",
        "The entry was removed from the keystore. Set the value again to restore the service.";

    // ---------------------------------------------------------------- presets & templates
    PRESET_IS_EXPORTED_JSON = "presetIsExportedJson",
        "A preset is the JSON that Settings → Presets exports.";
    PRESET_WRONG_FILE = "presetWrongFile",
        "Pointing the importer at another JSON file is the usual cause.";
    PRESET_TOO_NEW = "presetTooNew",
        "Update StackVo Desktop, or ask for a preset exported by an older version.";
    ONLY_SHIPPED_TEMPLATES = "onlyShippedTemplates",
        "Only the templates the app ships can be overridden.";
    REVERT_TEMPLATE_FIRST = "revertTemplateFirst",
        "Revert it first if you want the shipped version back.";

    // ---------------------------------------------------------------- profiling & debug
    PROFILE_IDS_FROM_LIST = "profileIdsFromList",
        "Profile ids are the cachegrind.out.* names from profile_list.";
    PROFILE_IS_COMPRESSED = "profileIsCompressed",
        "Xdebug compresses by default; StackVo turns that off when it enables profiling. \
         Re-record this profile, or gunzip the file yourself.";

    // ---------------------------------------------------------------- misc surfaces
    LOG_IDS_ARE_RELATIVE = "logIdsAreRelative",
        "Log ids are relative, with no parent or root segments.";
    INSTALL_A_TERMINAL = "installATerminal",
        "Install one, or use the built-in terminal instead.";
    CHOOSE_A_BROWSER = "chooseABrowser",
        "Choose a browser in Settings → External applications.";
    CHOOSE_AN_EDITOR = "chooseAnEditor",
        "Choose an editor in Settings, or open the folder manually.";
    WAIT_FOR_OPERATION = "waitForOperation",
        "Wait for it to finish, or watch the operation console for progress.";
    QUICK_COMMANDS_ARE_FIXED = "quickCommandsAreFixed",
        "Commands come from the fixed catalog; ids are not arbitrary.";
    IMAGE_REFERENCE_CHARSET = "imageReferenceCharset",
        "Lowercase letters, digits, and . _ - / : only.";
    COMPOSE_FILE_NOT_FOUND = "composeFileNotFound",
        "Looked for compose.yaml, compose.yml, docker-compose.yaml and docker-compose.yml.";
    COMPOSE_FILE_MUST_BE_VALID = "composeFileMustBeValid",
        "The file is resolved by `docker compose config`, so it has to be valid Compose — \
         including any variables it interpolates.";
    USE_GENERATE_RUN = "useGenerateRun",
        "Use generate_run; `verify` mode still reports drift against what is on disk.";
    MCP_NEEDS_ALLOW_WRITES = "mcpNeedsAllowWrites",
        "Restart it with --allow-writes to enable the writing tools.";
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// A duplicate key would make two different suggestions share one
    /// translation, and the second one to be written would win silently.
    #[test]
    fn every_key_is_unique() {
        let mut seen = HashSet::new();
        for hint in ALL {
            assert!(seen.insert(hint.key), "{} is declared twice", hint.key);
        }
    }

    /// The key is a locale-file path segment and an object key in JavaScript.
    /// A dot would nest it, a space would need quoting, and either would fail
    /// somewhere far away from here.
    #[test]
    fn keys_are_plain_camel_case_identifiers() {
        for hint in ALL {
            assert!(!hint.key.is_empty());
            assert!(
                hint.key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "{} is not usable as an object key",
                hint.key
            );
            assert!(
                hint.key
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_lowercase()),
                "{} should start lower case like every other locale key",
                hint.key
            );
        }
    }

    /// The English text is the fallback and the log line. An empty one would
    /// present as a hint that exists and says nothing.
    #[test]
    fn every_hint_says_something() {
        for hint in ALL {
            assert!(
                hint.english.trim().len() > 10,
                "{} has no usable English text",
                hint.key
            );
        }
    }
}
