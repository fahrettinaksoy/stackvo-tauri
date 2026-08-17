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
    REPL_RUNNER_NEEDS_FILES = "replRunnerNeedsFiles",
        "A runner is offered only where the project has the files it loads.";
    BUILD_AND_START_FOR_WORKER = "buildAndStartForWorker",
        "Build and start the project first — the worker runs its image.";
    WORKERS_ARE_DETECTED = "workersAreDetected",
        "Workers are detected from artisan and composer.json.";
    START_PROJECT_FOR_TUNNEL = "startProjectForTunnel",
        "Start the project first — the tunnel forwards to its container.";

    // ------------------------------------------------ worktrees (N)
    //
    // Three, not ten. Every refusal in `plan_worktree` is a whole sentence that
    // names the branch, the hostname or the directory that caused it — "give
    // the worktree a name of its own" says less than the message it would sit
    // under, and a hint that repeats the message is one people learn to skip.
    // What is catalogued here is the case where the message is the engine's or
    // git's own words and the way out is not in them.
    WORKTREE_IS_DIRTY = "worktreeIsDirty",
        "The worktree has uncommitted changes. Commit or stash them, or remove it with \
         Force, which discards them.";
    DATABASE_NAME_CHARSET = "databaseNameCharset",
        "Database names may contain lower-case letters, digits and underscore, and \
         must begin with a letter.";
    MONGO_HAS_NO_SOURCE_DATABASE = "mongoHasNoSourceDatabase",
        "Create the worktree with an empty database instead — MongoDB makes one on \
         the first write.";

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

    // ------------------------------------------------------- performance layer
    PERF_PATH_IS_RELATIVE = "perfPathIsRelative",
        "Name a directory inside the project, like vendor or storage/framework.";
    PERF_NOTHING_TO_SEED = "perfNothingToSeed",
        "That directory does not exist in the project yet. Install the dependencies first, \
         or enable it and let the tooling create it inside the container.";
    PERF_SEED_FAILED = "perfSeedFailed",
        "The directory could not be copied into the volume, so nothing was changed.";

    // ---------------------------------------------------------------- local DNS
    TLD_IS_ONE_LABEL = "tldIsOneLabel",
        "A suffix ends in one label of letters, digits and hyphens — stackvo.loc.";
    DNS_PLACE_THE_LINE_YOURSELF = "dnsPlaceTheLineYourself",
        "Add the line shown to whatever resolves names on this machine, then reload it.";
    DNS_START_THE_RESPONDER_FIRST = "dnsStartTheResponderFirst",
        "Start the responder first — this would otherwise point the machine at a closed port.";
    DNS_MACHINE_IS_NOT_ASKING_US = "dnsMachineIsNotAskingUs",
        "The responder answers, but this machine is not asking it. Something else may sit in \
         front of the resolver.";
    DNS_PUBLIC_NAMES_STOPPED = "dnsPublicNamesStopped",
        "The change took public names down with it and was undone. Nothing was left behind.";
    DNS_PORT_ALREADY_ANSWERING = "dnsPortAlreadyAnswering",
        "Something else on this machine is already answering on that port.";

    // ---------------------------------------------------------------- services
    SERVICE_MUST_BE_IN_CATALOG = "serviceMustBeInCatalog",
        "Only services listed in contracts/env.schema.json can be managed.";
    SUPPORTED_DATABASES = "supportedDatabases",
        "Supported: mysql, mariadb, postgres, mongo.";
    SNAPSHOT_NAME_CHARSET = "snapshotNameCharset",
        "Use letters, digits, dot, dash and underscore — the name becomes a filename. \
         `auto-` is reserved for scheduled snapshots.";
    SNAPSHOT_NAME_IN_USE = "snapshotNameInUse",
        "Choose another name, or delete the existing snapshot first — a snapshot is never \
         overwritten in place.";
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
    SETTING_IS_REQUIRED = "settingIsRequired",
        "The package marks this setting required — the service will not start without it.";
    PORT_HELD_BY_INSTANCE = "portHeldByInstance",
        "Another instance publishes this port. Change that one first, or pick another number.";
    PORT_IN_USE = "portInUse",
        "Something on this machine is already listening there. Pick another number.";
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

    // ---------------------------------------------------------------- assistants
    AGENT_CONFIG_UNPARSEABLE = "agentConfigUnparseable",
        "This file is not plain JSON — several editors allow comments in it, which cannot be \
         edited safely without deleting them. Open it and paste the block shown here.";
    BUILD_THE_MCP_SERVER = "buildTheMcpServer",
        "Build it first: `cargo build --release --bin stackvo-mcp` in the StackVo checkout.";
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
    MIGRATE_THE_WORKSPACE = "migrateTheWorkspace",
        "Move this workspace's services out of .env — the app offers it on the next launch, \
         and the Market page offers the same move. It is reversible.";
    SERVICE_PUBLISHES_NOTHING = "servicePublishesNothing",
        "Start the service, or check that it publishes a port — a container reachable only \
         on the Docker network has no address a client on this machine can use.";
    CHOOSE_A_DB_CLIENT = "chooseADbClient",
        "Install a client that opens this kind of address, or copy the connection string \
         and paste it in yourself.";
    WAIT_FOR_OPERATION = "waitForOperation",
        "Wait for it to finish, or watch the operation console for progress.";
    NO_REGISTRY_KEY = "noRegistryKey",
        "This build pins no registry key. An organisation running its own mirror can \
         pin one with the market.registryKey policy.";
    SIGNED_BY_UNKNOWN_KEY = "signedByUnknownKey",
        "The index may be from somewhere else, or the publisher may have rotated keys \
         without this machine learning the new one.";
    PACKAGE_VERSION_REVOKED = "packageVersionRevoked",
        "The publisher withdrew this version. Pick another, or read why in the \
         registry entry.";
    QUICK_COMMANDS_ARE_FIXED = "quickCommandsAreFixed",
        "Ids come from the built-in catalogue or from this project's own \
         stackvo.json; they are not arbitrary.";
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
    PORT_RANGE_EXHAUSTED = "portRangeExhausted",
        "Free a port near the one this service wants, or give the instance an explicit \
         port in its settings.";
    PACKAGE_PATHS_STAY_INSIDE = "packagePathsStayInside",
        "A package may only name files under its own directory.";
    PACKAGE_CONTENT_CHANGED = "packageContentChanged",
        "Reinstall the package; its files are not the ones the manifest was written for.";
    PACKAGE_NOT_INSTALLED = "packageNotInstalled",
        "Install the package for this version, or remove the instance that needs it.";
    PACKAGE_REFUSED_BY_POLICY = "packageRefusedByPolicy",
        "This package asks for something StackVo does not let a package have. Report it to \
         whoever published it.";
    PACKAGE_NOT_IN_REGISTRY = "packageNotInRegistry",
        "Refresh the catalogue, or pick a version it lists.";
    REGISTRY_WENT_BACKWARDS = "registryWentBackwards",
        "The catalogue this source serves is older than the one already here. Check the \
         source before using it.";
    REGISTRY_UNREACHABLE = "registryUnreachable",
        "The catalogue could not be fetched. Check the address and whether this machine \
         reaches it — a proxy set in the system settings is used.";
    REGISTRY_ADDRESS_IS_A_DIRECTORY = "registryAddressIsADirectory",
        "The address has to be the directory holding registry.json, not the page above it. \
         A GitHub repository URL is translated automatically; anything else is taken as given.";
    REGISTRY_MUST_BE_HTTPS = "registryMustBeHttps",
        "A catalogue address has to start with https://. Nothing verifies a signature yet, \
         so the transport is the whole of the protection.";
    REMOVE_THE_INSTANCE_FIRST = "removeTheInstanceFirst",
        "An instance is still using this package. Remove it, then uninstall.";
    SERVICE_IS_SINGLE_INSTANCE = "serviceIsSingleInstance",
        "This service runs one version at a time. Remove the instance you have first.";
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
