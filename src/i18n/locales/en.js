export default {
  $vuetify: {
    badge: 'badge',
    close: 'Close',
    dataIterator: { noResultsText: 'No matching records found', loadingText: 'Loading…' },
    noDataText: 'No data available',
  },

  app: {
    projects: 'Projects',
    services: 'Services',
    settings: 'Settings',
    refresh: 'Refresh',
    loading: 'Loading…',
    never: '—',
    cancel: 'Cancel',
    close: 'Close',
    copy: 'Copy',
    documentation: 'Documentation',
    buyMeCoffee: 'Buy me a coffee',
    socialMedia: 'Social media',
    language: 'Language',
    toggleTheme: 'Toggle theme',
  },

  close: {
    title: 'Close StackVo?',
    subtitle: 'Containers are managed by Docker and can keep running after the app closes.',
    tray: 'Minimize to tray',
    trayHint: 'The app stays in the background and the stack keeps running.',
    quit: 'Close, leave the stack running',
    quitHint: 'Quits the app without touching the containers.',
    stopAndQuit: 'Stop everything and close',
    stopAndQuitHint: 'Stops every StackVo container, then exits.',
    remember: "Don't ask again",
    behaviour: 'Close behaviour',
    behaviourHint: 'Choose what happens when you click the close button.',
    ask: 'Ask me every time',
  },
  nav: {
    dashboard: 'Dashboard',
    projects: 'Projects',
    services: 'Services',
    logs: 'Logs',
    dumps: 'Dumps',
    mail: 'Mail',
    settings: 'Settings',
    collapse: 'Collapse',
    expand: 'Expand',
  },

  system: {
    docker: 'Docker',
    running: 'Running',
    stopped: 'Stopped',
    containers: 'Containers',
  },

  /**
   * The tray icon and the native menu bar, both drawn by Rust.
   *
   * Only the strings with no home elsewhere are here — the tray's four
   * navigation entries come from `nav`, its engine words from `system`, and the
   * menu bar's three links from `about.links`, because those are the same
   * concepts and a second copy is a second thing to keep in step.
   *
   * The counted ones carry `{count}` / `{running}` / `{total}` rather than
   * being assembled in Rust, so a language that orders them differently needs
   * no code.
   */
  tray: {
    checking: 'Checking Docker…',
    show: 'Open StackVo',
    quit: 'Quit',
    engineDown: 'Docker is not running',
    engineUp: 'Docker running',
    noWorkspace: 'No StackVo directory selected',
    noProjects: 'No projects',
    containers: 'Containers: {count}',
    more: '+{count} more…',
    runningSummary: '{running}/{total} projects running',
    menuAbout: 'About StackVo',
  },

  quickActions: {
    startAll: 'Start all containers',
    stopAll: 'Stop all containers',
    restart: 'Restart all containers',
  },

  dashboard: {
    subtitle: 'Live state of the stack and the machine',
    title: 'Dashboard',
    overview: 'Overview',
    health: 'Health',
    projects: 'Projects',
    services: 'Services',
    images: 'Images',
    running: 'Running',
    stopped: 'Stopped',
    active: 'Active',
    inactive: 'Inactive',
    cpuLoad: 'CPU Load',
    cpuHistory: 'CPU Usage History',
    cpu: 'CPU',
    system: 'System',
    user: 'User',
    nice: 'Nice',
    idle: 'Idle',
    used: 'Used',
    available: 'Available',
    min: 'Min',
    avg: 'Avg',
    max: 'Max',
    diskIo: 'Disk I/O',
    diskIoSub: 'Real-time block device throughput',
    read: 'Read',
    write: 'Write',
    readHistory: 'Read History',
    writeHistory: 'Write History',
    network: 'Network Traffic',
    networkSub: 'Real-time network usage monitoring',
    downloadHistory: 'Download History',
    uploadHistory: 'Upload History',
    free: 'Free',
  },

  projectsView: {
    subtitle: 'The managed projects and their containers',
    title: 'Projects',
    list: 'Projects List',
    running: 'Running',
    searchPlaceholder: 'Search projects...',
    colDomain: 'Domain',
    colRuntime: 'Runtime',
    colServer: 'Server',
    colConfiguration: 'Configuration',
    colStopStart: 'Stop/Start',
    colRestart: 'Restart',
    colTerminal: 'Terminal',
    colOpen: 'Open in the browser',
    colDetail: 'Detail',
    colDelete: 'Delete',
    default: 'Default',
    noDnsRecord: 'No hosts entry',
    addToHosts: 'Add this line to your hosts file:',
  },

  servicesView: {
    disableTitle: 'Disable {name}?',
    disableBody: 'Nothing of this service is left behind. These are deleted:',
    disableContainer: 'The container (stopped, then removed)',
    disableVolumes: 'Its named volumes — including database contents, with no undo',
    disableImage: 'Its image, unless another container is using it',
    disableLogs: 'Its log directory under logs/services',
    disableHosts: 'The hosts entry for {domain} — an administrator password is asked for',
    disableConfirm: 'Disable and delete',
    hide: 'Hide the value',
    colDetail: 'Detail',
    serviceInfo: 'Service information',
    logInfo: 'Logs and mounts',
    ipAddress: 'IP address',
    network: 'Network',
    gateway: 'Gateway',
    portMappings: 'Port mappings',
    internal: 'internal only',
    connection: 'Connection string',
    connectionSubtitle:
      'A service has two addresses. The container name only resolves inside the Docker network — a client on this machine has to use the published port.',
    fromHost: 'From this machine',
    fromHostHint: 'Compass, TablePlus, psql',
    fromContainer: 'From another container',
    fromContainerHint: "your project's own application",
    notPublished:
      'The container is running but publishes no port to the host, so nothing on this machine can reach it.',
    credentials: 'Credentials',
    noCredentials: 'This service declares no credentials in .env.',
    reveal: 'Reveal the value',
    containerLogs: 'Container log',
    logPath: 'Log path',
    mount: 'Mount',
    noMounts: 'No mounts.',
    notCreated: 'The container has not been created yet.',
    subtitle: "The stack's shared infrastructure services",
    title: 'Services',
    list: 'Services List',
    searchPlaceholder: 'Search services...',
    colService: 'Service',
    colContainerName: 'Container Name',
    colDomain: 'Domain',
    colVersion: 'Version',
    colStopStart: 'Stop/Start',
    colRestart: 'Restart',
    colOpen: 'Open in the browser',
    colStatus: 'Status',
    enabled: 'ENABLED',
    disabled: 'DISABLED',
    networkInfo: 'Network Information',
    dependencies: 'Dependencies',
    noDependencies: 'No dependencies.',
    required: 'Required',
    optional: 'Optional',
  },

  projectDetail: {
    subtitle: 'One project: what it is running, what it is built from, and what it is doing now.',
    debug: 'Debugging',
    runtime: 'Runtime settings',
    title: 'Project Details',
    back: 'Back',
    indicator: 'Indicator',
    configuration: 'Configuration',
    container: 'Container',
    live: 'Live — resource metrics update every 2 seconds',
    disk: 'Disk',
    composition: 'Composition',
    usedShort: 'used',
    cpuActivity: 'CPU Activity',
    noHistory: 'No history yet — samples are taken once a minute.',
    noSample: 'no sample',
    less: 'Less',
    more: 'More',
    sslStatus: 'SSL Status',
    sslEnabled: 'Enabled (HTTPS)',
    type: 'Type',
    containerPath: 'Container Path',
    hostPath: 'Host Path',
    accessHttp: 'Access URL · HTTP',
    accessHttps: 'Access URL · HTTPS',
    phpExtensions: 'PHP Extensions',
    name: 'Name',
    uptime: 'Uptime',
    created: 'Created',
    restartPolicy: 'Restart Policy',
    restartCount: 'Restart Count',
    containerId: 'Container ID',
    imageSize: 'Image Size',
    dnsHosts: 'DNS (HOSTS)',
    configured: 'Configured',
    gateway: 'Gateway',
    portMappings: 'Port Mappings',
    notPublished: 'not published',
    copied: 'Copied',
    applyToContainer: 'Recreate the container',
  },

  workspace: {
    none: 'No project directory selected yet.',
    change: 'Change',
    source: {
      stored: 'saved choice',
      env: 'STACKVO_PROJECTS',
      migrated: 'carried over from an older install',
      none: 'not selected',
    },
    version: 'Version',
    appDir: 'App directory',
    appDirDesc:
      'Everything StackVo produces lives here: compose files, logs, certificates, settings. Created automatically, never asked about.',
  },

  engine: {
    title: 'Docker engine',
    running: 'Running',
    down: 'Not running',
    socket: 'Socket',
    context: 'Context',
    version: 'Version',
    apiVersion: 'API version',
    platform: {
      'docker-desktop': 'Docker Desktop',
      colima: 'Colima',
      orbstack: 'OrbStack',
      engine: 'Docker Engine',
      unknown: 'Unknown',
    },
  },

  stats: {
    cpu: 'CPU',
    memory: 'Memory',
    storage: 'Storage',
    network: 'Network',
    cores: 'cores',
    download: 'Download',
    upload: 'Upload',
    inUse: 'in use',
    unused: 'unused',
  },

  projects: {
    searchPlaceholder: 'Search projects…',
    openDetail: 'Open detail',
    openSite: 'Open site',
    title: 'Projects',
    empty: 'No projects yet',
    emptyText:
      'Your project directory holds nothing StackVo manages. Create one, or move an existing folder here and adopt it.',
    noMatch: 'No matching projects',
    noMatchText: 'Nothing matched “{term}”.',
    clearSearch: 'Clear search',
    running: 'Running',
    stopped: 'Stopped',
    notBuilt: 'Not built',
    domainMissing: 'no hosts entry',
    domainMissingHint: 'This domain is missing from /etc/hosts, so the browser cannot reach it.',
    invalidManifest: 'Invalid stackvo.json',
    problems: 'problem',
    manifestChanged: 'stackvo.json changed — regenerate to apply it.',
    openFolder: 'Open folder',
  },

  services: {
    hostPort: 'Host port',
    unmetDependency: 'Unmet dependency',
  },

  console: {
    doneToast: '{operation} finished — {duration}',
    failedToast: '{operation} failed — the console has the output',
  },

  bootstrap: {
    title: 'Setting the stack up',
    subtitle:
      'A one-time setup: the compose files get written and the core containers come up. When it finishes, stackvo.loc is serving.',
    generate: 'Writing the compose files',
    generateDetail:
      'Templates rendered with your settings — these are the files every up is given.',
    start: 'Starting the core containers',
    startDetail: 'Traefik, the proxy every domain goes through. The first run may pull an image.',
    certificates: 'Issuing the certificate',
    certificatesDetail:
      'Traefik serves HTTPS, and without a certificate no domain answers at all. The first run may ask for your password.',
    trust: 'Trusting the certificate',
    trustDetail:
      'macOS grants this only interactively, so a terminal opens and asks for your sudo password. Skip it and the stack still runs — the browser just warns.',
    waitingForPassword: 'A terminal is open — type your password there; this is watching for it.',
    retry: 'Try again',
    untrusted:
      'The certificate was issued but this machine does not trust the issuer — the browser will warn. You can retry from Settings → Certificates.',
  },

  preflight: {
    title: 'StackVo is not ready to run',
    subtitle: '{count} requirements are not met. The app opens once they are.',
    recheck: 'Check again',
    blocked: 'Cannot be checked until a requirement above it is met.',
    lead: 'Work through the steps in order — the marked one has a button that does it for you.',
    progress: '{done} of {total} steps done',
    nextStep: 'Next step',
    manual: 'This step has to be done by hand.',
    help: 'Installation instructions',

    workspace: 'Project directory',
    workspaceHint: {
      macos:
        'Choose the folder your projects live in — an existing one is fine, so is a new one. It has to be somewhere Docker can reach; anywhere under your home directory is safe. StackVo keeps its own files in ~/.stackvo, not here.',
      linux:
        'Choose the folder your projects live in — an existing one is fine, so is a new one. It has to be somewhere Docker can reach; anywhere under your home directory is safe. StackVo keeps its own files in ~/.stackvo, not here.',
      windows:
        'Choose the folder your projects live in — an existing one is fine, so is a new one. It has to be on a drive Docker Desktop shares. StackVo keeps its own files in its own directory, not here.',
    },
    workspaceAction: 'Choose project directory',
    workspaceInstalled: 'Projects will be read from {path}.',

    engine: 'Docker engine',
    engineHint: {
      macos: 'Docker Desktop, OrbStack or Colima is not running. Start opens Docker Desktop.',
      linux:
        'The Docker daemon is not running. Start tries systemd; if it needs rights, run `sudo systemctl start docker`.',
      windows:
        'Docker Desktop is not running. Start opens it; it needs the WSL2 backend installed.',
    },
    engineAction: 'Start',

    compose: 'Docker Compose v2',
    composeHint: {
      macos: 'The app drives compose profiles, which arrived in v2. Update Docker Desktop.',
      linux:
        'The app drives compose profiles, which arrived in v2. Install the docker-compose-plugin package.',
      windows: 'The app drives compose profiles, which arrived in v2. Update Docker Desktop.',
    },

    network: 'Shared Docker network',
    networkHint: {
      macos: 'The generated compose files declare it external, so compose will not create it.',
      linux: 'The generated compose files declare it external, so compose will not create it.',
      windows: 'The generated compose files declare it external, so compose will not create it.',
    },
    networkAction: 'Create network',

    hosts: 'Hosts file entries',
    hostsHint: {
      macos:
        'These names are not in /etc/hosts, so the browser cannot resolve any of them. Adding them asks for an administrator password; what gets written is shown first.',
      linux:
        'These names are not in /etc/hosts, so the browser cannot resolve any of them. Adding them asks for an administrator password; what gets written is shown first.',
      windows:
        'These names are not in Windows\\System32\\drivers\\etc\\hosts, so the browser cannot resolve any of them. Adding them asks for administrator rights; what gets written is shown first.',
    },
    hostsAction: 'Add entries',

    mkcert: 'mkcert',
    mkcertHint: {
      macos:
        'SSL is on, so every domain is served over HTTPS. Without mkcert the certificate is not issued and browsers refuse the site. Install it with `brew install mkcert`.',
      linux:
        'SSL is on, so every domain is served over HTTPS. Without mkcert the certificate is not issued and browsers refuse the site. Install it from your package manager, then run `mkcert -install`.',
      windows:
        'SSL is on, so every domain is served over HTTPS. Without mkcert the certificate is not issued and browsers refuse the site. Install it with `choco install mkcert`.',
    },
  },
  adopt: {
    found: '{n} folder(s) under projects/ have no stackvo.json.',
    from: 'detected from {files}',
    noEvidence: 'nothing recognisable — defaults will be used',
    action: 'Adopt',
  },
  migrate: {
    read: 'Read compose',
    title: 'Import {name} from its compose file',
    project: 'The project',
    field: {
      runtime: 'Runtime',
      server: 'Server',
      phpVersion: 'PHP version',
      nodeVersion: 'Node version',
      documentRoot: 'Document root',
      domain: 'Domain',
      extensions: 'PHP extensions',
    },
    services: 'Services to enable',
    servicesAlready: 'The services this project needs are already enabled.',
    unmapped: 'No StackVo equivalent — you will need to handle these yourself:',
    alreadyManaged: 'This project already has a stackvo.json; only the services will be changed.',
    evidence: 'What each answer was read from',
    manifest: 'The stackvo.json this would write',
    apply: 'Import',
  },
  mail: {
    subtitle: 'Mail your projects sent, caught before it left the machine.',
    inbox: 'Inbox',
    title: 'Mail',
    unread: '{n} unread',
    select: 'Select a message to read it.',
    fromLabel: 'From',
    toLabel: 'To',
    replyToLabel: 'Reply-To',
    offHeadline: 'The mail catcher is off',
    stoppedHeadline: 'The mail catcher is stopped',
    emptyHeadline: 'No mail yet',
    preview: 'Preview',
    text: 'Text',
    source: 'Source',
    headersTab: 'Headers',
    attachmentsTab: 'Attachments',
    compatTab: 'Compatibility',
    linksTab: 'Links',
    save: 'Save',
    // `{'@'}` is vue-i18n's literal escape: a bare `@` starts a linked-message
    // reference, so this logged "Invalid linked format" on every render and fell
    // back to the raw string. Caught by the compilation gate in
    // `tests/i18n.spec.js`.
    searchPlaceholder: 'Search — from:a{\'@\'}b.c subject:"invoice"',
    matching: '{n} matching',
    compatSupported: 'fully supported across {n} mail-client features',
    compatLegend: 'Green fully supported · amber partial · red unsupported.',
    compatWarning: '{category} · appears {found}×',
    compatClean: 'Nothing in this markup is unsupported anywhere tested.',
    checkLinks: 'Check links',
    linksHint: 'Fetches every link in the message — this leaves your machine.',
    noLinks: 'No links in this message.',
    enablePrompt:
      'The mail service is not enabled. Captured mail appears here as your app sends it — enable it now?',
    enableAction: 'Enable {service}',
    startAction: 'Start {service}',
    enabling:
      'Enabling — writing .env, regenerating, and starting the container. The first run downloads the image, so give it a minute.',
    count: '{n} captured',
    empty: 'Nothing has been sent yet.',
    noSubject: '(no subject)',
    notRunning: 'The mail catcher is not running, so nothing is being captured.',
    clear: 'Empty inbox',
    deleteOne: 'Delete this message',
    confirmClear:
      'This deletes every captured message. A mail catcher is a bin, so there is no backup.',
  },
  db: {
    title: 'Backup',
    subtitle: 'Dump and restore the {db} database.',
    subtitleAll: 'Dump and restore every database on this server.',
    notRunning: 'The container is not running, so there is nothing to read from.',
    dump: 'Back up',
    restore: 'Restore',
    dumped: 'Written to {path}',
    restored: 'Restored from {path}',
    confirmRestore:
      'This replaces the contents of {db} with the contents of the chosen file. Anything currently in it is lost.',
  },
  xdebug: {
    title: 'Xdebug',
    subtitle: 'Step debugging for this project.',
    on: 'Enabled',
    off: 'Disabled',
    needsRebuild:
      'The extension is compiled into the image, so this does nothing until the project is regenerated and rebuilt.',
    notActive:
      'The running container does not carry the Xdebug settings. Restart the project to apply them.',
    active: 'Live in the running container — set a breakpoint and load the site.',
    ideSettings: 'IDE settings',
    port: 'Port',
    ideKey: 'IDE key',
    serverName: 'Server name (PHP_IDE_CONFIG)',
    pathMapping: 'Path mapping',
    version: 'Xdebug version',
    cliCaveat:
      'Note: `stackvo up` from the command line does not layer this configuration, and will recreate the container without it.',
  },
  stackPreset: {
    export: 'Export this stack',
    exportDesc:
      'Writes which services are enabled and at which versions to a small JSON file, safe to commit. Passwords are not in it — the format has nowhere to put them.',
    name: 'Preset name',
    namePlaceholder: 'e.g. team-backend',
    saveFile: 'Save file…',
    summary: '{enabled} of {total} services enabled.',
    preview: 'What the file will contain',
    import: 'Import a preset',
    importDesc:
      'Shows exactly what would change before anything is written. Your passwords and ports are never touched.',
    chooseFile: 'Choose a file…',
    untitled: 'Untitled preset',
    colSubject: 'What',
    colFrom: 'Now',
    colTo: 'After',
    absent: 'not set',
    apply: 'Apply {n} changes',
    applied: 'Applied.',
    alreadyMatches: 'This stack already matches the preset — {n} settings checked, none differ.',
    nothingUsable: 'Nothing in this preset applies to this version of StackVo.',
    rejected: 'Not applied:',
    thenRegenerate:
      'Enabling a service changes what the generator emits — regenerate the configuration, then bring the stack up.',
  },

  dumps: {
    source: { web: 'Web', cli: 'CLI', queue: 'Queue' },
    regex: 'Regular expression',
    filterSource: 'Filter by source',
    copy: 'Copy what is shown',
    copyValue: 'Copy the value',
    pause: 'Pause',
    resume: 'Resume',
    resumeHint: 'Resume — {n} new',
    clearHint: 'Clear the list and the recorded events',
    capturingCount: '{on} of {total} projects capturing.',
    needsRecreateShort: 'The container has to be recreated',
    allDescription: 'dump() and dd() from every project that is capturing',
    noProjects: 'No PHP project can carry the bridge.',
    allProjects: 'All projects',
    capture: 'Catch dump() and dd()',
    captureHint: 'Takes effect immediately — no container is touched.',
    help: 'About this pane',
    captureOff: 'Capture is off. Switch it on and dump() output collects here.',
    search: 'Search',
    title: 'Dumps',
    explain:
      'Catches dump() and dd() out of the response and shows them here instead. Symfony’s own dump server does the rendering, inside your project’s container.',
    needsRecreate:
      'The running container does not have the dump settings yet. They are fixed when a container is created, so restarting is not enough — the container has to be recreated.',
    clear: 'Clear',
    waiting: 'Waiting for a dump… call dump() anywhere in the app.',
    ddEndsTheRequest:
      'dump() lets the request continue. dd() takes the dump and ends it, and Symfony marks that as a 500 — so a dump appearing here while the browser shows an error is expected.',
  },

  release: {
    load: 'Load a bundle',
    loadExplain:
      'Read a .tar written by Save back into this machine’s Docker. This is the receiving end of an air-gapped hand-off, so it needs no project and no plan.',
    loaded: 'Docker adopted:',
    title: 'Production image',
    explain:
      'A deployable image built from the one this project already runs — same PHP version, same extensions, same web server. Not a copy of it: the development image has no application code (the source is mounted from your disk) and carries Xdebug.',
    tag: 'Image tag',
    tagHint: 'Built from {base}',
    build: 'Build',
    excluded: 'Kept out of the image',
    dockerfile: 'The Dockerfile this will use',
    checked: 'What the built image actually contains',
    clean: '{tag} is ready. Checked by running it, not by reading the Dockerfile.',
    notClean: 'This image is not safe to ship yet.',
    leaked: 'Environment files are in the image: {files}',
    noEnv: 'No environment file — supply configuration when you run it.',
    xdebugOn: 'Xdebug is still active. Do not deploy this.',
    xdebugOff: 'Xdebug is not active.',
    noApp: 'The image has no application files.',
    save: 'Save as a tarball…',
  },

  profiler: {
    title: 'Profiler',
    explain:
      'Xdebug’s own profiler, recorded into files this app reads. No account and no extra extension — it is the same Xdebug that does the step debugging.',
    needsXdebug: 'Turn Xdebug on first — profiling is a mode of the same extension.',
    modeDebug: 'Step debugging',
    modeProfile: 'Profiling',
    modesExclusive:
      'One or the other. Stepping connects on every request; profiling waits for a trigger, so leaving both on would break one of them.',
    howToRecord:
      'Nothing is recorded until a request asks for it. Add ?{trigger}=1 to the URL, or set it as a cookie.',
    modeMismatch: 'The container is in “{running}” mode; the setting says “{wanted}”.',
    needsRecreate:
      'The running container does not have this yet. Environment and mounts are fixed when a container is created, so restarting is not enough — the container has to be recreated.',
    recorded: 'Recorded profiles ({n})',
    noneYet: 'Nothing recorded yet.',
    clear: 'Delete all ({size})',
    compressed: 'gzipped',
    open: 'Open',
    deleteOne: 'Delete this profile',
    summary: '{n} functions · {total} of measured work · {creator}',
    truncated:
      'This profile was larger than the read limit, so the numbers below cover only part of it.',
    colFunction: 'Function',
    colSelf: 'Own time',
    colInclusive: 'With calls',
    colCalls: 'Calls',
  },

  quickCmd: {
    title: 'Commands',
    explain:
      'The commands you run in this project, without opening a terminal and remembering the container name. Only what the project has the files for is offered.',
    because: 'from {file}',
    opensTerminal: 'opens a terminal',
    needsRunning: 'These run inside the project’s container. Start it first.',
    none: 'No artisan, composer.json, package.json or wp-config.php here, so there is nothing to offer.',
  },

  devServer: {
    title: 'Dev server',
    explain:
      'Runs the project’s dev server with your source mounted live, instead of the production build baked into the image. Without this the container holds a copy of the code taken when it was built, so editing a file changes nothing.',
    on: 'On — source mounted, dev server running',
    off: 'Off — production build from the image',
    command: 'Dev command',
    commandHint: 'Replaces the production command, which is: {production}',
    live: 'Live. Save a file and the browser follows.',
    needsRecreate:
      'Dev mode is on but the running container was created without the source mount. Bring the project up again.',
    projectConfig: 'Your project also needs this',
    projectConfigWhy:
      'This part lives in your repository, so it is shown rather than written. Vite answers 403 to a domain its config does not name, and its hot-reload client has to be told the port the browser is really on — behind the proxy that is 443, not the dev server’s own port.',
    notAllowed: '{file} does not mention this — requests to this domain will come back 403.',
    configured: 'Your config already handles this.',
    noAdvice:
      'No Vite, Nuxt or Next found in package.json, so there is no config advice to give — the source mount still applies.',
    modulesNote:
      'node_modules stays in its own volume so the mount does not hide the install the image did for Linux. After changing dependencies, rebuild the project.',
    cliCaveat:
      'Note: `stackvo up` from the command line does not layer this, and will recreate the container in production mode.',
  },

  phpIni: {
    title: 'PHP settings',
    explain:
      'Overrides for this project, written to .stackvo/php.ini and mounted read-only into PHP’s conf.d — parsed after PHP’s own php.ini, so what is set here wins. Safe to edit by hand and safe to commit.',
    field: {
      memory_limit: 'Memory limit',
      upload_max_filesize: 'Max upload size',
      post_max_size: 'Max POST size',
      max_execution_time: 'Max execution time',
    },
    // The placeholder is measured from the running container, never a
    // documented default: these images ship no php.ini at all, and
    // max_execution_time is 0 under FPM rather than the 30 the manual lists.
    notMeasured: 'not set',
    measured: 'Placeholders are what PHP in the running container reports now.',
    hint: {
      memory_limit: 'A number with K, M or G. -1 for unlimited.',
      upload_max_filesize: 'Capped by the POST size, whichever is smaller.',
      post_max_size: 'Should be at least the upload size.',
      max_execution_time: 'Whole seconds. 0 for unlimited.',
    },
    save: 'Save',
    removeFile: 'Remove the file',
    emptyRemoves: 'An empty field removes the directive.',
    needsRestart:
      'Saved. PHP reads its configuration at start-up — restart the project to apply it.',
    needsRecreate:
      'The file is on disk but the running container has no mount for it. Bring the project up again to add it.',
    unmanaged: 'Other directives in this file',
    file: 'File',
    mountedAt: 'Mounted at',
    cliCaveat:
      'Note: `stackvo up` from the command line does not layer this mount, and will recreate the container without it.',
  },
  certs: {
    title: 'HTTPS certificate',
    subtitle: 'One wildcard certificate covers the dashboard, every service and every project.',
    sslOff:
      'SSL_ENABLE is off in .env, so the stack is served over HTTP and no certificate is used.',
    current: 'Up to date',
    stale: 'Needs reissuing',
    caTrusted: 'CA trusted',
    caUntrusted: 'CA not trusted',
    caUnknown: 'CA trust unknown',
    expiresOn: 'Expires {date} ({days} days)',
    expiredOn: 'Expired on {date}',
    noMkcert: 'mkcert is not installed, so the certificate cannot be issued or reissued.',
    missing: 'Not covered — these domains will show a browser warning',
    dropping: 'Will be dropped on the next reissue',
    rejected: 'Skipped — not valid hostnames',
    covered: 'Covered ({n})',
    reissue: 'Reissue certificate',
    trustInTerminal: 'Trust the CA (in a terminal)',
    trustInTerminalHint:
      'macOS grants the authorization for trust settings only interactively, so a windowed app cannot do it. This opens your terminal and asks for your sudo password. Then quit and reopen the browser.',
    leafLabel: 'Certificate',
    caLabel: 'Signing CA',
    whySeparate:
      'They are in separate directories because the certificate directory is mounted into the Traefik container. With the CA private key in there, anything in that container could issue a certificate for any domain this machine trusts. The CA is also never reissued — losing it costs every trust decision you have made.',
    notReloaded:
      'The certificate was reissued, but the proxy is still serving the previous one. Restart the stack, or run generate, to pick it up.',
  },
  serviceSettings: {
    pick: 'Pick a service',
    title: 'Services',
    sectionDesc: 'Each service’s own .env settings.',
    desc: 'Pick a service to edit what it is configured with. Applying rebuilds its container, because a running one keeps the environment it was created with.',
    all: 'All',
    fields: {
      VERSION: 'Version',
      URL: 'Subdomain',
      HOST_PORT: 'Host port',
      PORT: 'Port',
      HOST: 'Host',
      DATABASE: 'Database',
      DB: 'Database',
      USER: 'Username',
      PASSWORD: 'Password',
      ROOT_PASSWORD: 'Root password',
      ADMIN_USER: 'Admin username',
      ADMIN_USERNAME: 'Admin username',
      ADMIN_PASSWORD: 'Admin password',
      ADMIN_PASS: 'Admin password',
      DEFAULT_USER: 'Default user',
      DEFAULT_PASS: 'Default password',
      DEFAULT_PASSWORD: 'Default password',
      DEFAULT_EMAIL: 'Default email',
      BASICAUTH_USERNAME: 'Basic auth username',
      BASICAUTH_PASSWORD: 'Basic auth password',
      INITDB_ROOT_USERNAME: 'Initial root username',
      INITDB_ROOT_PASSWORD: 'Initial root password',
      UPLOAD_LIMIT: 'Upload limit',
      CLUSTER_NAME: 'Cluster name',
    },
    categories: {
      databases: 'Databases',
      cache: 'Cache',
      queue: 'Queues',
      search: 'Search',
      monitoring: 'Monitoring',
      devtools: 'Developer tools',
      adminUis: 'Admin UIs',
    },
    off: 'Off',
    empty: 'No services in this category.',
    none: 'This service has no settings of its own.',
    default: 'default',
    reveal: 'Reveal',
    hide: 'Hide',
    showKey: 'Show the .env key ({key})',
    apply: 'Apply and rebuild',
    confirmTitle: 'Rebuild the container?',
    confirmBody:
      'Saving these is not enough on its own: {service} is running with the environment it was created with, so its container will be stopped and recreated with the new values.',
    confirmApply: 'Apply',
  },
  about: {
    tagline: 'Local development environments, managed as a stack.',
    system: 'System information',
    systemDesc: 'What a bug report needs. Copy it rather than retyping it.',
    appVersion: 'StackVo',
    os: 'Operating system',
    docker: 'Docker',
    context: 'Docker context',
    workspace: 'Workspace',
    copy: 'Copy',
    copied: 'Copied',
    resources: 'Resources',
    resourcesDesc: 'Opens in your browser.',
    links: {
      docs: 'Documentation',
      source: 'Source code',
      issues: 'Report an issue',
      sponsor: 'Buy me a coffee',
    },
    copyright: 'MIT licensed · © 2026 Fahrettin Aksoy',
    licences: 'Third-party licences',
    licencesDesc: 'The notices this build ships with, exactly as compiled in.',
    licencesFailed: 'The licence notice could not be read from this build.',
    close: 'Close',
  },
  settings: {
    servers: {
      gzipTypesHint: 'Space-separated MIME types. Empty leaves nginx’s own list.',
      field: {
        SERVER_MAX_BODY_SIZE: 'Max body size',
        SERVER_CLIENT_BODY_TIMEOUT: 'Client body timeout',
        SERVER_KEEPALIVE_TIMEOUT: 'KeepAlive timeout',
        SERVER_FASTCGI_CONNECT_TIMEOUT: 'FastCGI connect timeout',
        SERVER_FASTCGI_SEND_TIMEOUT: 'FastCGI send timeout',
        SERVER_FASTCGI_TIMEOUT: 'FastCGI read timeout',
        SERVER_TCP_NODELAY: 'TCP nodelay',
        SERVER_GZIP: 'Gzip',
        SERVER_GZIP_COMP_LEVEL: 'Gzip level',
        SERVER_GZIP_TYPES: 'Gzip types',
      },
      extra: 'Extra directives',
      extraDesc:
        'Added to every generated config for this server. Comments and blank lines are dropped, so a file of nothing but notes changes nothing.',
      extraPlaceholder: 'client_body_timeout 120s;',
      // `{'…'}` is vue-i18n's literal escape. Without it the compiler reads
      // `{{ VAR }}` as a nested placeholder, logs "Not allowed nest
      // placeholder" on every render and falls back to the raw string — the
      // text survives, the console noise does not, and noise is what hides a
      // real error.
      extraHint: "{'{{ VAR }}'} is substituted from .env. Takes effect on the next generate.",
      title: 'Web servers',
      desc: 'What the server in front of PHP will accept.',
      limits: 'Request limits',
      limitsDesc:
        'Written into the generated server config. Left at the default, nothing is written at all.',
      sizeInvalid: 'A number, optionally followed by k, m or g.',
      secondsInvalid: 'Whole seconds.',
      phpNote:
        'An upload is refused by whichever limit is lowest. PHP has its own — upload_max_filesize, post_max_size and memory_limit — and those are per project, under the project’s PHP settings.',
      applies: 'Where this applies',
      appliesDesc: 'Not every server is configured through a file.',
      supportNote:
        'Apache is configured inside its own Dockerfile and Swoole by an inline script, so neither has a file to add directives to. The request limits above reach nginx and caddy only — FrankenPHP’s Caddyfile does not carry them, so directives are all it takes.',
    },
    defaults: {
      title: 'Project defaults',
      desc: 'What a new project starts with, whichever runtime it uses.',
      runtimes: 'Runtime versions',
      php: 'PHP and web server',
      phpTools: 'PHP build',
    },
    workspaceAndControl: 'Directory and control',
    workspaceAndControlDesc: 'Where this stack lives, how it is run, and how it is shared.',
    groups: {
      app: 'Application',
      workspace: 'Workspace',
      stack: 'Stack',
      help: 'Help',
    },
    subtitle: 'Application preferences',

    // Appearance section.
    appearance: 'Appearance',
    appearanceSectionDesc: 'Customise the theme, accent, neutral palette and corner radius.',
    themeColors: 'Theme and colours',
    themeColorsDesc: 'Personalise how the app looks',
    primaryColor: 'Accent colour',
    neutralPalette: 'Neutral palette',
    radius: 'Corner radius ({px}px)',
    resetAppearance: 'Defaults',
    typography: 'Typography and legibility',
    typographyDesc: 'Typeface, interface scale and contrast',
    fontFamily: 'Typeface',
    fontFamilyHint: 'Only faces the system already has are listed.',
    uiScale: 'Interface scale ({px}px)',
    highContrast: 'High contrast',
    highContrastHint: 'Strengthens secondary text and dividers.',
    reduceMotion: 'Reduce motion',
    density: 'Interface density',
    densityCompact: 'Tight',
    densityComfortable: 'Comfortable',
    densitySpacious: 'Spacious',
    systemAccent: 'System colour',
    reduceMotionHint: 'Turns transitions off; progress indicators keep spinning.',
    statusColors: 'Status colours',
    statusColorsDesc: 'Which colours mean running, stopped and failed',
    statusPalette: 'Palette',
    statusPalettes: {
      default: 'Default (green / red)',
      colorblind: 'Colour-blind safe (Okabe-Ito)',
      muted: 'Muted',
    },
    darkConsoles: 'Keep consoles dark',
    darkConsolesHint: 'Log and terminal panels stay dark in the light theme too.',
    presets: 'Presets',
    presetsDesc: 'Name a look and come back to it in one click',
    presetName: 'Preset name',
    savePreset: 'Save',
    noPresets: 'No presets saved yet.',
    neutrals: {
      graphite: 'Graphite',
      carbon: 'Carbon',
      midnight: 'Midnight',
      forest: 'Forest',
      warm: 'Warm grey',
    },
    fonts: {
      system: 'System',
      grotesk: 'Grotesk (Helvetica)',
      serif: 'Serif (Georgia)',
      mono: 'Monospace',
    },

    // Localisation section.
    localisation: 'Localisation',
    localisationDesc: 'Interface language and writing direction.',
    languageDesc: 'Language of the interface and the tray menu',
    consoleLanguage: 'Console language',
    consoleLanguageDesc: 'Language of the log and terminal panels',
    consoleLanguageHint: 'Keeps shared output readable regardless of your interface language.',
    consoleFollowsApp: 'Same as the interface',
    direction: 'Writing direction',
    directionDesc: 'Which way the interface flows',
    rtl: 'Right-to-left layout',
    rtlHint: 'Mirrors every component; for trying Arabic and Hebrew layouts.',

    // Section descriptions: what each pane is for, said once on entry.
    preferencesDesc: 'Appearance, language, external apps and close behaviour.',
    certificates: 'Certificates',
    certificatesDesc: 'The HTTPS certificate, the domains it covers and the CA behind it.',
    aboutDesc: 'Version, signed updates and diagnostics.',

    // Groups.
    workspaceGroup: 'Working directory',
    workspaceGroupDesc: 'The checkout this app drives',

    templates: {
      title: 'Template overrides',
      description:
        'The templates live inside the app. A file appears in the workspace only when you take it over — and from then on, updates no longer reach it.',
      count: '{count} of {total} templates are overridden in this workspace.',
      none: 'All {total} templates are read from the shipped versions.',
      pick: 'Template to take over',
      pickHint: 'The file is copied into the workspace and opened in your editor.',
      override: 'Take over and edit',
      open: 'Open',
      revert: 'Back to shipped',
      revertTitle: 'Delete the overridden template?',
      revertBody:
        'Your edited file is deleted and the shipped version takes over. There is no other copy of your edit — this cannot be undone.',
      reload: 'Reload',
    },
    engineGroupDesc: 'State of the engine running the containers',
    externalApps: 'External apps',
    externalAppsDesc: 'Which app terminals and editors open in',
    startup: 'Startup and shutdown',
    startupDesc: 'What happens when the app opens and closes',
    compose: 'Containers',
    generatorDesc: "Compares the Rust generator's output against the Bash one",
    updatesDesc: 'Signed release check and install',

    theme: 'Theme',
    language: 'Language',
    preferences: 'Preferences',
    stackSub: 'Compose level: regenerates and recreates containers.',
    runtimes: {
      desc: 'The version a new project starts on, per runtime. Which versions exist is the app’s own catalog, not a setting.',
    },
    php: {
      versionDesc:
        'What a new PHP project starts with. Existing projects keep the version recorded in their own stackvo.json.',
      version: 'PHP version',
      versionHint: 'Preselected in the new-project form; each project can still choose its own.',
      server: 'Web server',
      serverHint: 'Serves PHP projects. Other runtimes run their own dev server instead.',
      composer: 'Composer version',
      composerHint:
        'Installed into the PHP image. "latest" tracks the current release at build time.',
      nodejs: 'Node.js version',
      nodejsHint:
        'For asset builds inside the PHP container — separate from a Node project runtime.',
    },
    secrets: {
      title: 'Where credentials are kept',
      description:
        'Database passwords, tokens and server ids can live in this machine’s keystore instead of in .env.',
      whatItDoes:
        'Moving a credential stores it in Keychain, Credential Manager or the Secret Service, and leaves a reference in .env. The value is no longer in the file that gets backed up, synced and pasted into support threads.',
      stillGenerated:
        'It is still written into generated/docker-compose.dynamic.yml, which is where Compose reads it from. This takes the password out of .env; it does not take it off the disk.',
      cliCannotRead:
        'The stackvo.sh command-line tool cannot read these. If you use it on this workspace, leave the credentials in .env.',
      noKeystore: 'This machine has no keystore this app can reach, so nothing can be moved.',
      unresolvable:
        'These credentials point at the keystore and it did not answer. Generating files is blocked until they resolve — unlock your keychain, or restore the value.',
      none: 'This workspace has no credentials set.',
      inKeystore: 'In the keystore',
      inEnvFile: 'In .env, in plain text',
      move: 'Move',
      restore: 'Restore',
    },
    policy: {
      title: 'This machine is managed',
      body: 'A policy file on this machine sets {count} setting(s). Values it locks cannot be changed here.',
      source: 'Policy file:',
      registry: 'Images are pulled through:',
      notASecurityBoundary:
        'A policy file tells this app what your organisation intends. It is not a security boundary — it can be redirected with STACKVO_POLICY_FILE.',
      brokenTitle: 'The policy file did not fully apply',
      brokenBody:
        'Nothing was applied from the parts below, and the rest of the app is running as if unmanaged. Whoever deployed this file probably believes it is in force.',
      managed: 'Managed',
      managedHint: 'This value comes from a policy file on this machine.',
      locked: 'Locked',
      lockedHint: 'A policy file sets this value and does not allow it to be changed here.',
    },
    shape: {
      title: 'Domain and network',
      sectionDesc: 'Where projects are addressed and how they are served.',
      suffixRequired: 'A suffix is required; routes are built from it.',
      suffixInvalid: 'Letters, digits, dots and hyphens only, starting and ending with one.',
      network: 'Docker network',
      networkHint:
        'The network every service joins. Renaming it recreates containers on the next up.',
      networkRequired: 'A network name is required.',
      networkInvalid: 'Letters, digits, dots, hyphens and underscores only.',
      reset: 'Back to the default',
      addressTitle: 'Addresses',
      addressDesc:
        'Where projects and services answer. Every hostname sits under this suffix, which is what lets one certificate cover them all.',
      suffixLabel: 'Namespace',
      suffixLabelHint:
        'Groups every address under one parent. Optional — leave it empty to use the TLD alone.',
      suffixTld: 'Extension',
      suffixTldHint:
        '.test and .localhost are reserved for local use. .dev is a real TLD and needs HTTPS.',
      preview: 'Addresses become:',
      suffixHsts:
        'This extension is on the browsers’ HSTS preload list: nothing under it loads over plain HTTP, with no way to click through. Turn on HTTPS below before using it.',
      networkTitle: 'Network and TLS',
      networkGroupDesc:
        'Which Docker network services share, and whether they are served over HTTPS.',
      thenRegenerate:
        'Saved. Regenerate so the routing labels pick this up — until then the stack still answers on the old ones.',
      thenCertificates:
        'A new suffix needs its own certificate; check the Certificates pane afterwards. Existing projects keep the domain recorded in their own stackvo.json.',
      regenerate: 'Regenerate',
      ssl: 'Serve over HTTPS',
      sslHint: 'Issues and mounts local certificates for the domain suffix above.',
      sslOffBreaksRouting:
        'With HTTPS off, no HTTPS entry point is generated — but every route still targets it, so no project or service domain will resolve until it is back on.',
      proxyTitle: 'Reverse proxy',
      proxyDesc:
        'Traefik. Every project and admin UI is reached through it, and it terminates TLS — which is what the HTTPS switch above turns on.',
      proxyPorts: 'Published ports',
      proxyDashboard: 'Open the dashboard',
      hostsTitle: 'Hosts file',
      hostsDesc:
        'Every domain here is resolved by name, so each needs a line in /etc/hosts. Changing it asks for your password.',
      hostsFix: 'Fix all',
      hostsOk: 'All resolved',
      hostsManual: 'added by hand',
      hostsStale: 'Written by StackVo and no longer needed — removed by the same button:',
      redirect: 'Redirect HTTP to HTTPS',
      redirectHint: 'Plain requests are answered with a redirect instead of the site.',
      redirectBlocked: 'Needs HTTPS on — redirecting to a scheme that is off leads nowhere.',
      phpDesc:
        'What a new PHP container is built with. Changing these affects projects generated from now on.',
      tools: 'Tools',
      toolsHint: 'Installed alongside PHP. Type to add, click the cross to remove.',
      apt: 'System packages',
      aptHint: 'Installed with apt inside the container.',
    },
    about: 'About',
    diagnostics: 'Application log',
    diagnosticsHint:
      'StackVo’s own diagnostic record — not your projects’ server logs. Attach this folder when reporting a problem.',
    openLogs: 'Open folder',
    logsUnavailable: 'No writable log location was found on this system.',
    logsRedacted: 'Password and token values are masked as the log is written.',
    saveBundle: 'Save a diagnostic bundle',
    saveBundleHint:
      'One archive with the log, the startup checks, the doctor report and any crash reports — everything a bug report needs, instead of the log alone.',
    saveBundleDone: 'Saved ({bytes}). It is plain text inside; have a look before sending it.',
    verifyNow: 'Verify the generator now',
    checkForUpdates: 'Check for updates',
    updates: 'Updates',
    version: 'Version',
    upToDate: 'Up to date.',
    updateAvailable: 'Version {version} is available.',
    installUpdate: 'Install and restart',
    updaterUnconfigured:
      'This build cannot verify updates: it has no public key compiled in. Update checks stay off until the release signing key is configured.',
    updateSigned: 'The bundle signature is verified against the key compiled into this build.',
    generator: 'Generator (drift check)',
    generatorReady: 'the disk matches what the generator writes',
    generatorDiffers: 'drift — a generated file was changed by hand or is stale',
    themeSystem: 'System',
    themeLight: 'Light',
    themeDark: 'Dark',
    terminalApp: 'Terminal',
    editorApp: 'Code editor',
    browserApp: 'Browser',
    browserAppHint: 'Used by every “visit” button — project and service domains open here.',
    appsHint: 'Applications that are not installed cannot be selected.',
    appDefault: 'Default',
    startMinimized: 'Start minimized to tray',
    autostart: 'Start at login',
    save: 'Save {count} change(s)',
    saved: 'Saved',
  },

  a11y: {
    copy: 'Copy to clipboard',
    moreActions: 'More actions',
    followOutput: 'Follow output',
    stopFollowing: 'Stop following output',
    toggleConsole: 'Toggle console',
    // Announced by a screen reader while a metric card waits for its first
    // sample. Vuetify gives the spinner role="progressbar" and no name.
    loading: 'Loading',
    close: 'Close',
  },
  actions: {
    start: 'Start the container',
    stop: 'Stop the container',
    restart: 'Restart the container',
    build: 'Build the project',
    generate: 'Regenerate the configuration',
    up: 'Bring the stack up',
    down: 'Stop the stack',
    composeRestart: 'Restart the stack',
  },

  logs: {
    title: 'Logs',
    live: 'live',
    openInEditor: 'Open this file in the editor',
    waiting: 'Waiting for output…',
    liveFrom: 'live from here',
    regex: 'Regular expression',
    pause: 'Pause',
    resume: 'Resume',
    resumeHint: 'Resume — {n} line(s) held',
    clear: 'Clear',
    clearHint: 'Clear the view — nothing is deleted from disk',
    containerStream: 'Container output',
    // The cross-project tail. Live only, so an empty pane is its opening state
    // and not a fault — the wording has to say that outright.
    allDescription:
      'A live tail across every project. Only output written from now on appears here — open a project to read the history of one file.',
    allProjects: 'Every project',
    waitingAll: 'Watching. Lines appear as your projects write them.',
    following: 'following {followed} of {total} files · {projects} projects',
    files: '{n} files',
    group: {
      application: 'Application',
      server: 'Server',
    },
    search: 'Search',
    filterLevel: 'Filter by level',
    clearFilter: 'Clear filter',
    copy: 'Copy what is shown',
    noMatch: 'Nothing matches — {n} lines hidden.',
    showing: 'Showing {shown} of {total}',
    level: {
      debug: 'Debug',
      info: 'Info',
      notice: 'Notice',
      warning: 'Warning',
      error: 'Error',
      critical: 'Critical',
    },
  },

  hosts: {
    title: 'Update the hosts file',
    explain:
      'Project domains need a hosts entry to open in a browser. Only lines inside the StackVo marker block are rewritten; the rest of the file is left untouched.',
    elevation:
      'This asks for your administrator password. Nothing is written until you approve the change.',
    noChange: 'No change needed — the entries are already there.',
    fix: 'Add entry',
    apply: 'Apply',
    cancel: 'Cancel',
  },

  terminal: {
    title: 'Terminal',
    explain:
      'A shell inside this project’s container, in the window. The system terminal is still one click away in the header — this is for a quick look without leaving the page.',
    needsRunning: 'Start the project first — a shell runs inside its container.',
    start: 'Open a shell',
    stop: 'Close',
    exited: 'The shell exited ({code}).',
  },
  workers: {
    title: 'Workers',
    explain:
      'Queue and scheduler processes, run as containers built from this project’s own image — same PHP, same extensions, same .env. Docker restarts a crashed worker on its own (unless-stopped), whether or not this app is open.',
    none: 'No artisan file found — workers are detected from Laravel’s files.',
    needsRunning: 'Start the project first — a worker runs the project’s built image.',
    queue: 'Queue worker',
    queueDesc:
      'php artisan queue:work — processes queued jobs; restarts hourly so it never serves stale code for long.',
    scheduler: 'Scheduler',
    schedulerDesc:
      'php artisan schedule:work — runs scheduled tasks in the foreground; no host cron entry needed.',
    horizon: 'Horizon',
    horizonDesc:
      'php artisan horizon — Laravel Horizon supervisor, offered because composer.json requires it.',
    start: 'Start',
    stop: 'Stop',
    restarts:
      'Docker has restarted this worker {count} time(s) — check its logs if this keeps climbing.',
  },

  tunnel: {
    title: 'Share',
    explain:
      'A temporary public URL that forwards to this project — for webhook senders (Stripe, GitHub) that cannot reach a .loc domain. Runs a Cloudflare quick tunnel as a sidecar container on the stack network; no account needed.',
    needsRunning: 'Start the project first — the tunnel forwards to its container.',
    start: 'Get a public URL',
    startHint:
      'The first start downloads the cloudflared image. The URL is random, lives only while the tunnel runs, and changes on every start.',
    connecting: 'Connecting — Cloudflare is assigning the URL…',
    stop: 'Stop sharing',
    publicWarning:
      'This URL is live on the public internet and has no authentication. Anyone who has it reaches this project on your machine. Stop sharing when the test is done.',
  },

  doctor: {
    title: 'Doctor',
    sectionDesc: 'What is wrong, said with names — and the repair beside each finding.',
    loading: 'Examining the stack…',

    requirements: 'Startup requirements',
    requirementsDesc: 'The same checks that gate the first screen, re-checkable from here.',

    coreTitle: 'Core containers',
    coreDesc:
      'Every project and service domain is routed through these. With them down nothing answers by name, however correct the install is.',
    coreRunning: 'Running.',
    coreStopped: 'The container exists but is stopped.',
    coreMissing: 'No container at all — the stack was never started, or was taken down.',
    coreUnknown: 'Docker is not running, so this cannot be read.',
    coreStart: 'Start the core stack',

    portsTitle: 'Host ports',
    portsDesc: 'Every port the generated stack will claim, and who holds it right now.',
    portsNone: 'The generated stack publishes no host ports — run the generator first.',
    portFree: 'Free.',
    portOurs: 'Held by the stack itself ({name}).',
    portHeld: 'In use by {process}.',
    portHeldPid: 'In use by {process} (pid {pid}).',
    portHeldUnknown: 'In use, but the process could not be identified.',
    portUnknown: 'The listener table could not be read.',

    hostsTitle: 'Hosts file',
    hostsDesc: 'A project domain without a hosts entry is a site the browser cannot find.',
    hostsOk: 'Every project domain has an entry.',
    extTitle: 'PHP extensions',
    extDesc:
      'The generator skips an extension it cannot install and says nothing, so the failure turns up later as a fatal “undefined function”.',
    extOk: 'Every selected extension can build.',
    extDefault: '“{ext}” is in the default selection but cannot build — {detail}.',
    extDefaultWhy: 'A new project created now would be missing it. Checked against PHP {versions}.',
    extProject: '“{ext}” cannot build in {project}.',
    extOpen: 'Open project',
    extRemove: 'Remove it',
    extRemoveHint: 'Nothing that runs changes — the build already drops it.',
    hostsMissing: '{count} domain(s) have no hosts entry.',
    hostsRepair: 'Review & repair',

    generatedTitle: 'Generated configuration',
    generatedDesc:
      'The compose files are derived from .env and the project manifests. Edit an input without regenerating and the stack runs yesterday’s config.',
    generatedOk: 'Up to date with its inputs.',
    generatedStale: 'Older than {file} — the stack is running yesterday’s config.',
    generatedMissing: 'Never generated.',
    generatedUnknown: 'Cannot be checked without a workspace.',
    regenerate: 'Regenerate',

    spaceTitle: 'Disk',
    spaceDesc: 'Every rebuild leaves a dangling image behind, and this app rebuilds a lot.',
    spaceUnknown: 'Cannot be read while the engine is down.',
    spaceImages: '{count} unused image(s)',
    spaceVolumes: '{count} unused volume(s)',
    reclaim: 'Reclaim space…',
    pruneTitle: 'Reclaim disk space',
    pruneImagesLabel: 'Remove {count} dangling image(s) — {size}. Rebuildable by definition.',
    pruneVolumesLabel: 'Remove {count} unused volume(s) — {size}.',
    pruneVolumesWarning:
      '“Unused” means “not currently mounted” — the data of a stopped project qualifies. Anything removed here is gone; back up databases first.',
    pruneBuildCacheLabel: 'Remove the whole build cache.',
    pruneBuildCacheWarning:
      'Deleting a project already reclaims the cache its own image held. What is left is shared: every project image builds from the same PHP base and the same extension installs. Removing it costs no data — it costs every project a full rebuild next time.',
    pruneConfirm: 'Remove',
    pruneResult:
      'Removed {images} image(s), {volumes} volume(s) and {caches} cache record(s) — {size} reclaimed.',

    ownersTitle: 'Who holds the bytes',
    ownerCol: 'Member',
    ownerImage: 'Image',
    ownerImageSize: 'Image size',
    ownerRw: 'Writable layer',
    ownerShared: 'shared upstream image',
    ownerOrphan: 'orphaned build',
  },

  newProject: {
    nameHint:
      'Lower-case, starting with a letter or digit; dash, underscore and dot allowed (e.g. api.myapp).',
    domainHint: 'Generated from the project name when left empty.',
    domain_https:
      "This TLD is on the browsers' HSTS preload list: it only loads over HTTPS, with no way to click through. Turn on HTTPS in Settings first.",
    domain_certificate:
      'Outside the configured suffix, so the wildcard certificate does not cover it — reissue certificates after creating the project.',
    documentRootHint: 'Path relative to the project root.',
    portHint: 'The port the app listens on inside the container.',
    sectionProject: 'Project',
    sectionPhp: 'PHP configuration',
    sectionNode: 'Node configuration',
    sectionLang: '{runtime} configuration',
    langVersion: 'Version',
    optionalStep: 'Optional — clear it to skip this step.',
    langBindHint: 'Must listen on 0.0.0.0 and the port above; Traefik proxies to it.',
    title: 'New project',
    name: 'Project name',
    template: 'Start from',
    templates: {
      empty: 'Empty project',
      git: 'Clone a git repository',
      laravel: 'Laravel',
      wordpress: 'WordPress',
      symfony: 'Symfony',
      nextjs: 'Next.js',
      nuxt: 'Nuxt',
      vue: 'Vue (Vite)',
      react: 'React (Vite)',
      svelte: 'SvelteKit',
      astro: 'Astro',
      cakephp: 'CakePHP',
      yii: 'Yii 2',
      codeigniter: 'CodeIgniter 4',
      laminas: 'Laminas (Zend)',
      drupal: 'Drupal',
      prestashop: 'PrestaShop',
      django: 'Django',
      rails: 'Ruby on Rails',
      slim: 'Slim',
      nest: 'NestJS',
      tina: 'TinaCMS',
      angular: 'Angular',
      typo3: 'TYPO3',
      gin: 'Gin',
      echo: 'Echo',
      flask: 'Flask',
      fastapi: 'FastAPI',
      sinatra: 'Sinatra',
      rocket: 'Rocket',
    },
    templateGroups: {
      php: 'PHP',
      node: 'JavaScript',
      cms: 'CMS & e-commerce',
      python: 'Python',
      go: 'Go',
      other: 'Ruby & Rust',
    },
    detectedHint:
      'The runtime, web server and document root come from the files the installer writes — Laravel serves from public/, WordPress from the project root. They are editable afterwards in the project’s settings.',
    templateHint:
      'The framework’s own installer runs in a throwaway container, then detection configures the project from what it wrote. The first run downloads the installer image — give it a few minutes.',
    gitUrl: 'Repository URL',
    gitUrlPlaceholder: "git{'@'}server.example.com:group/subgroup/repo.git",
    gitUrlHint: 'An SSH or HTTPS clone URL. Any host — including your own GitLab.',
    gitAuthHint:
      'Cloning uses the git on this machine. Your keys, ssh config and server permissions come from your own setup — StackVo manages none of them. A URL that works in your terminal works here.',
    gitManifestHint:
      'If the repository has a stackvo.json, its settings are used as they are — the team’s answer wins and the fields above are ignored. If it has none, the project is configured from what the clone contains.',
    domain: 'Domain',
    runtime: 'Runtime',
    phpVersion: 'PHP version',
    nodeVersion: 'Node version',
    server: 'Web server',
    documentRoot: 'Document root',
    extensions: 'PHP extensions',
    incompatible: 'Cannot be installed on this PHP version',
    tooManyExtensions: 'more extensions than the catalog offers',
    install: 'Install command',
    build: 'Build command (optional)',
    start: 'Start command',
    port: 'Port',
    bindHint: 'Must bind 0.0.0.0, or Traefik cannot reach it.',
    create: 'Create',
    unavailableRuntimes: 'Hidden — no generator: {list}',
    deleteTitle: 'Delete {name}?',
    deleteBody: 'The project leaves the StackVo list. Your source files stay on disk.',
    // Said before the button is pressed, because these are not recoverable and
    // the old dialog mentioned none of them.
    deleteAlso:
      'Its container, image, generated Dockerfile, logs, hosts entry and certificate name are removed with it.',
    deleteFiles: 'Also delete the project folder (cannot be undone)',
    delete: 'Delete',
  },

  projectSettings: {
    title: 'Configure {name}',
    open: 'Configure',
    nameLocked: 'The folder name is the project’s identity; renaming means moving the folder.',
    extensionUnknown: 'Requested by this project, not in the catalogue',
    domainChanged:
      'The hosts entry and the certificate still name the old domain. Both are offered once the change is applied.',
    applyPending:
      'Saved. The container still runs the previous configuration until the files are regenerated and the image rebuilt.',
    applyNow: 'Apply now',
    saveAndApply: 'Save & apply',
    engineDown: 'Docker is not running, so nothing can be rebuilt. Save keeps the change on disk.',
  },

  detail: {
    openFolder: 'Open folder',
    dockerfileDesc: 'How the Rust generator renders this project — without writing the file.',
    compatHint:
      'Reproduces what Bash writes today; extensions that cannot build are dropped silently.',
    strictHint: 'Refuses to render when an extension cannot build, and says which one.',
    notBuilt: 'The container has not been built yet; build it to stream logs.',
    openInEditor: 'Open in editor',
    externalTerminal: 'Open in external terminal',
    manifest: 'Manifest',
    manifestHint: 'stackvo.json — saving reorders keys to satisfy the write rules.',
    save: 'Save',
    bringUp: 'Bring up via compose',
    dockerfile: 'Dockerfile',
    image: 'Image',
    state: 'State',
    matchesBash: 'Identical to the Bash output',
    differsFromBash: 'Differs from the Bash output',
    strict: 'Strict',
    compat: 'Compat',
    silentlySkipped: 'Bash drops these without saying so',
  },

  // Suggestions, keyed by `hintKey` on the error the Rust side raised.
  //
  // The catalogue is `src-tauri/src/hints.rs`; these are its translations, and
  // `src-tauri/tests/hint_translations.rs` fails the build if the two sets ever
  // differ in either direction — a hint with no translation, or a translation
  // for a hint nothing raises any more.
  //
  // The English in en.js is a copy of what the Rust carries as its fallback,
  // and the same test pins the two equal. Deliberate: it turns an edit to the
  // English into a change that has to pass through the translations, instead of
  // one that silently leaves Turkish describing the old behaviour.
  errorHints: {
    startDocker: 'Start Docker Desktop and try again.',
    startDockerOrSetHost: 'Start Docker Desktop, or set DOCKER_HOST if the engine is elsewhere.',
    startDockerManually: 'Start Docker manually, then retry.',
    projectMayNotBeBuilt: 'The project may not be built yet.',
    chooseWorkspace: 'Choose an empty folder for StackVo to set up, or one it already manages.',
    projectNameCharset:
      'Names may contain letters, digits, dot, underscore and dash, and must start with a letter or digit.',
    pathLeavesProjects: 'Refusing to operate on a path that leaves projects/.',
    onlyProjectFolders: 'Only project folders inside the selected workspace can be opened.',
    adoptInstead: 'Adopt it instead — that is the path that writes one.',
    fixOrAdopt: 'Fix the file, or delete it and adopt the folder instead.',
    runDoctorThenRetry:
      'Settings → Doctor lists what is wrong and can repair it; then clone or register again.',
    adoptExistingCode: 'Use adoption for existing code — scaffolding is for a brand-new project.',
    chooseAnotherName: 'Choose another name, or adopt the folder that is already there.',
    installGitOrAdopt: 'Install git, or clone the repository yourself and adopt the folder.',
    editFromManifestTab: "Edit it from the project's Manifest tab instead.",
    startProjectForCommands: 'Start the project first — these commands run inside its container.',
    buildAndStartForWorker: 'Build and start the project first — the worker runs its image.',
    workersAreDetected: 'Workers are detected from artisan and composer.json.',
    startProjectForTunnel: 'Start the project first — the tunnel forwards to its container.',
    installMkcert:
      'Install it with `brew install mkcert` (macOS), your package manager (Linux), or `choco install mkcert` (Windows), then try again.',
    checkTldAndDomains: 'Check DEFAULT_TLD_SUFFIX in .env and the `domain` in each stackvo.json.',
    certificateIssuedButUntrusted:
      'The certificate is issued either way and the stack serves — the browser warns about the issuer until the authority is trusted. Settings → Certificates has a button that does it in your terminal, where the password prompt can be answered.',
    runMkcertInstall:
      'Run `mkcert -install` once in a terminal — it needs a password for the system trust store, and a windowed app has no terminal to ask in.',
    hostnameCharset: 'Hostnames may contain letters, digits, dots and hyphens.',
    hostsNeedsAdmin: 'Administrator rights are required to edit the hosts file.',
    hostsNotReplaced: 'The hosts file could not be replaced.',
    installPolkit: 'Install polkit, or edit /etc/hosts manually.',
    serviceMustBeInCatalog: 'Only services listed in contracts/env.schema.json can be managed.',
    supportedDatabases: 'Supported: mysql, mariadb, postgres, mongo.',
    enableAMailCatcher: 'Enable mailhog (or mailpit) in .env, then regenerate.',
    mailUiMayBeStarting: 'The container may still be starting, or its UI port may be taken.',
    envKeyCharset: 'Keys must match ^[A-Z_][A-Z0-9_]*$ so Compose can interpolate them.',
    envIsOneKeyPerLine:
      'The .env format is one key per line; multi-line values cannot be read back.',
    revealValueFirst: 'Reveal the value first, or leave the field untouched.',
    phpIniDirectiveCharset: 'Directive names are letters, digits, underscores and dots.',
    phpIniIsOnePerLine: 'php.ini is one directive per line.',
    phpIniSizeFormat:
      'Sizes are a number with an optional K, M or G — 256M, 1G, 512. Times are whole seconds. -1 means unlimited.',
    serverDirectivesUnsupported:
      'Only nginx, caddy and frankenphp have a generated config to add directives to.',
    unlockTheKeystore:
      'Unlock your keychain and try again — the password for this setting is stored there.',
    onlyCredentialsMove: 'Only passwords, tokens and server ids can be kept in the keystore.',
    keystoreEntryIsGone:
      'The entry was removed from the keystore. Set the value again to restore the service.',
    settingIsManaged:
      'This value comes from a policy file on this machine. Ask whoever administers it.',
    presetIsExportedJson: 'A preset is the JSON that Settings → Presets exports.',
    presetWrongFile: 'Pointing the importer at another JSON file is the usual cause.',
    presetTooNew: 'Update StackVo Desktop, or ask for a preset exported by an older version.',
    onlyShippedTemplates: 'Only the templates the app ships can be overridden.',
    revertTemplateFirst: 'Revert it first if you want the shipped version back.',
    profileIdsFromList: 'Profile ids are the cachegrind.out.* names from profile_list.',
    profileIsCompressed:
      'Xdebug compresses by default; StackVo turns that off when it enables profiling. Re-record this profile, or gunzip the file yourself.',
    logIdsAreRelative: 'Log ids are relative, with no parent or root segments.',
    installATerminal: 'Install one, or use the built-in terminal instead.',
    chooseABrowser: 'Choose a browser in Settings → External applications.',
    chooseAnEditor: 'Choose an editor in Settings, or open the folder manually.',
    waitForOperation: 'Wait for it to finish, or watch the operation console for progress.',
    quickCommandsAreFixed: 'Commands come from the fixed catalog; ids are not arbitrary.',
    imageReferenceCharset: 'Lowercase letters, digits, and . _ - / : only.',
    composeFileNotFound:
      'Looked for compose.yaml, compose.yml, docker-compose.yaml and docker-compose.yml.',
    composeFileMustBeValid:
      'The file is resolved by `docker compose config`, so it has to be valid Compose — including any variables it interpolates.',
    useGenerateRun: 'Use generate_run; `verify` mode still reports drift against what is on disk.',
    mcpNeedsAllowWrites: 'Restart it with --allow-writes to enable the writing tools.',
  },

  errors: {
    ENGINE_UNREACHABLE: 'Cannot reach the Docker engine.',
    NO_WORKSPACE: 'No StackVo directory selected.',
    // The code covers every filesystem failure — reading, writing, removing —
    // and the sentence under it names the operation. A headline that says
    // "read" over a message about removing a directory contradicts it.
    IO_ERROR: 'A filesystem operation failed.',
    NOT_FOUND: 'Not found.',
    ALREADY_EXISTS: 'A project with that name already exists.',
    INVALID_INPUT: 'The input is not valid.',
    INVALID_MANIFEST: 'stackvo.json does not satisfy the contract.',
    UNSUPPORTED: 'Not supported in v1.',
    GENERATE_FAILED: 'Generation failed.',
    BUILD_FAILED: 'The build failed.',
    PERMISSION_DENIED: 'Permission was not granted.',
    // Deliberately worded so it does not read as something to retry. The
    // headline above PERMISSION_DENIED invites another attempt with a password;
    // this one never can be, and saying so is the whole difference.
    FORBIDDEN: 'A policy on this machine does not allow this.',
    CONFLICT: 'That operation is already running.',
    UNKNOWN: 'Something went wrong.',
  },
};
