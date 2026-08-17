/**
 * GENERATED — do not edit. `node tools/generate-types.mjs`
 *
 * The IPC surface as types, from `contracts/ipc.json`. §3 #10.
 *
 * Applies to plain JavaScript: an editor reads this beside `ipc.js` and offers
 * the argument names, the return shape and a complaint about a method that does
 * not exist. There is no compiler in this project and this does not add one —
 * `tools/generate-types.mjs` says what that would take and why it is separate.
 *
 * Measured at generation: 101 named types, 242 wrappers, 24 field(s) the
 * contract's prose could not be read as a type (typed `unknown`).
 */

export interface Adoptable {
    /** string */
    name: string;
    /** string */
    path: string;
    /** Detected */
    detected: Detected;
    /** bool */
    has_files: boolean;
    /** string? */
    compose_file?: string;
}

export interface Catalog {
    runtimes: Record<string, unknown>;
    /** string[] */
    servers: string[];
    /** string */
    default_server: string;
    php_extensions: Record<string, unknown>;
}

export interface CertPlan {
    /** string[] */
    add: string[];
    /** string[] */
    remove: string[];
    /** string[] */
    domains: string[];
    /** string[] */
    covered: string[];
    /** string[] */
    rejected: string[];
    /** bool */
    changed: boolean;
    /** string */
    cert_path: string;
    /** bool */
    install_ca: boolean;
    /** bool (cert_apply only) */
    reloaded: boolean;
}

export interface CertStatus {
    /** bool */
    ssl_enabled: boolean;
    /** bool */
    mkcert_available: boolean;
    /** string? */
    mkcert_version?: string;
    /** string? */
    ca_root?: string;
    /** string? */
    ca_path?: string;
    /** bool? */
    ca_trusted?: boolean;
    /** string? */
    cert_path?: string;
    /** string? */
    key_path?: string;
    /** string? */
    not_after?: string;
    /** number? */
    days_remaining?: number;
    /** bool */
    expired: boolean;
    /** string[] */
    covered: string[];
    /** string[] */
    required: string[];
    /** string[] */
    missing: string[];
    /** bool */
    stale: boolean;
}

export interface CompanionRow {
    /** string — the manifest's handle, and the suffix of the container name */
    name: string;
    /** string — stackvo-<instance>-<name>, derived exactly as render::context derives it */
    containerName: string;
    /** string — the reference actually pulled, digest-pinned when the manifest pins one */
    image: string;
    /** bool */
    built: boolean;
    /** bool */
    running: boolean;
    /** string? — same vocabulary as Service.health */
    health?: string;
}

export interface Connection {
    /** string */
    service: string;
    /** 'mysql'|'postgres'|'mongo'|'redis'|'memcached'|'amqp'|'http'|'hostport'|'smtp' */
    kind: 'mysql' | 'postgres' | 'mongo' | 'redis' | 'memcached' | 'amqp' | 'http' | 'hostport' | 'smtp';
    /** Endpoint? */
    fromHost?: Endpoint;
    /** Endpoint */
    fromContainer: Endpoint;
    /** bool */
    masked: boolean;
    /** string? */
    passwordKey?: string;
}

export interface ContainerStats {
    /** f32 */
    cpu_percent: number;
    /** u64 */
    memory_used: number;
    /** u64 */
    memory_limit: number;
    /** f32 */
    memory_percent: number;
    /** u64 */
    net_rx: number;
    /** u64 */
    net_tx: number;
}

export interface Credential {
    /** string — without the SERVICE_<ID>_ prefix */
    key: string;
    /** string */
    envKey: string;
    /** string — masked when secret */
    value: string;
    /** bool */
    secret: boolean;
}

export interface DbTarget {
    /** string */
    service: string;
    /** 'mysql'|'mariadb'|'postgres'|'mongo' */
    kind: 'mysql' | 'mariadb' | 'postgres' | 'mongo';
    /** string */
    container: string;
    /** string? */
    database?: string;
    /** string? */
    user?: string;
    /** bool */
    enabled: boolean;
    /** bool */
    running: boolean;
    /** string */
    extension: string;
}

export interface DeclaredPort {
    /** string — the manifest's handle: main, console, smtp */
    name: string;
    /** u16 */
    container: number;
    /**
     * u16? — what the container publishes when there is one, the recorded allocation when there is not
     */
    host?: number;
    /** 'tcp'|'udp' */
    protocol: 'tcp' | 'udp';
}

export interface DependencyReport {
    /** string */
    service: string;
    /** string */
    description: string;
    dependencies: Record<string, unknown>;
    /** bool */
    has_unmet_dependencies: boolean;
    /** string[] */
    internal: string[];
}

export interface DependencyRow {
    /** string — what the manifest asks for, so MariaDB can answer 'sql' */
    capability: string;
    /** string? — the one service that will do, when only one will */
    service?: string;
    /** string? — the installed instance answering it, null when none does */
    provider?: string;
    /** bool */
    required: boolean;
    /** bool — false whenever there is no provider; the two are told apart by `provider` */
    running: boolean;
}

export interface Detected {
    /** string? */
    framework?: string;
    /** 'php'|'node' */
    runtime: 'php' | 'node';
    /** string */
    server: string;
    /** string? */
    document_root?: string;
    /** string? */
    php_version?: string;
    /** string? */
    node_version?: string;
    /** number? */
    node_port?: number;
    /** string? */
    node_start?: string;
    /** 'certain'|'likely'|'guess' */
    confidence: 'certain' | 'likely' | 'guess';
    /** string[] */
    evidence: string[];
}

export interface DevServerStatus {
    /** bool */
    supported: boolean;
    /** bool */
    enabled: boolean;
    /** 'vite'|'nuxt'|'next'|'unknown' */
    tool: 'vite' | 'nuxt' | 'next' | 'unknown';
    /** string */
    command: string;
    /** string? */
    production_command?: string;
    /** bool? */
    mounted?: boolean;
    /** bool */
    running: boolean;
    /** bool */
    needs_recreate: boolean;
    /** string? */
    config_file?: string;
    /** bool? */
    host_allowed?: boolean;
    /** string? */
    snippet?: string;
    /** string? */
    domain?: string;
    /** number */
    port: number;
    /** string */
    overlay_path: string;
}

export interface DiskOwner {
    /** string */
    id: string;
    /** string? */
    image?: string;
    /** u64 */
    imageSize: number;
    /** bool */
    imageDedicated: boolean;
    /** u64 */
    containerRw: number;
    /** bool */
    running: boolean;
}

export interface DockerfilePreview {
    /** string */
    project: string;
    /** string */
    runtime: string;
    /** string? */
    server?: string;
    /** string */
    dockerfile: string;
    /** [{extension, reason}] — what the Bash generator drops without telling anyone */
    skipped: Record<string, unknown>;
    /** string */
    bashOutputPath: string;
    /** bool */
    matchesBashOutput: boolean;
}

export interface Flame {
    /** Frame[] — folded stacks; a function under two callers is two frames */
    frames: unknown;
    /** number — microseconds accounted for, which is the width of the root row */
    total: number;
    /** number — entry and exit records read */
    records: number;
    /** number — distinct stacks the file held */
    stacks: number;
    /** bool — the file was longer than the reader's ceiling; this is the start of the request */
    truncated: boolean;
    /** number — paths too thin to draw, dropped and counted rather than silently missing */
    pruned: number;
    /** bool — the stack went deeper than 64 frames */
    depthCapped: boolean;
}

export interface Doctor {
    /** Preflight */
    preflight: Preflight;
    ports: Record<string, unknown>;
    /** string[] */
    hostsMissing: string[];
    /**
     * { suffix: string, port: number }? — null in every ordinary state, INCLUDING the feature being off. Set only when the machine's resolver points at a local responder that is not answering: the one DNS failure nothing else on screen reports, where the app, the containers and the proxy all look healthy and every project domain fails to resolve.
     */
    dns?: Record<string, unknown>;
    generated: { state: 'ok' | 'warn' | 'fail' | 'unknown'; detail?: string };
    /** SystemResources? */
    space?: SystemResources;
    extensions: Record<string, unknown>;
}

export interface DumpsStatus {
    /** bool */
    available: boolean;
    /** bool */
    configured: boolean;
    /** string */
    binary: string;
    /** string */
    address: string;
    /** string */
    runtime: string;
}

export interface Endpoint {
    /** string */
    uri: string;
    /** string */
    host: string;
    /** number */
    port: number;
}

export interface EngineStatus {
    /** bool */
    reachable: boolean;
    /** string? */
    version?: string;
    /** string? */
    api_version?: string;
    /** string? */
    context?: string;
    /** 'docker-desktop'|'colima'|'orbstack'|'engine'|'unknown' */
    platform: 'docker-desktop' | 'colima' | 'orbstack' | 'engine' | 'unknown';
    /** string? */
    socket_path?: string;
    /** string? */
    error?: string;
}

export interface FanoutStream {
    /** string */
    stream_id: string;
    /** number */
    followed: number;
    /** number */
    total: number;
    /** number */
    projects: number;
}

export interface FinishedEvent {
    /** string */
    operation_id: string;
    /** string */
    subject: string;
    /** bool */
    success: boolean;
    /** u64 */
    duration_ms: number;
    /** string? */
    error?: string;
    /** string? */
    log_path?: string;
}

export interface GeneratorReport {
    /**
     * [{ file, path, status: 'match'|'differ'|'missing'|'error', firstDifferenceLine?, error? }]
     */
    files: Record<string, unknown>;
    /** u32 */
    matched: number;
    /** u32 */
    differed: number;
    /** bool */
    readyToTakeOver: boolean;
    /** string[] — configuration problems the port can see and StackVo does not report, e.g. C-20 */
    warnings: string[];
}

export interface HandoverInstance {
    /** string */
    id: string;
    /** string */
    service: string;
    /** string */
    version: string;
    /** map<string, u16> */
    ports: Record<string, unknown>;
    /** map<string, string> */
    volumes: Record<string, unknown>;
}

export interface HandoverNote {
    /**
     * string (resolvedMovingTag | portMoved | adoptedVolume | settingHasNoHome | unknownService | versionNotInstalled | nothingToInstall | noFreePort). `versionNotInstalled` is NOT 'unpublished': the catalogue handover reads is the local package tree, so the honest sentence is 'not installed here', and the two ask for different things — one is a click in the Market, the other is a version that was never in the index.
     */
    kind: string;
    /** string */
    subject: string;
    /** string */
    detail: string;
}

export interface HandoverPreview {
    /** bool */
    pending: boolean;
    /** bool */
    migrated: boolean;
    /** HandoverInstance[] */
    instances: HandoverInstance[];
    /** HandoverNote[] */
    notes: HandoverNote[];
    /** HandoverNote[] */
    blockers: HandoverNote[];
    /** bool */
    backup: boolean;
    /** MissingPackage[] — packages the handover needs and this machine does not have */
    missing: MissingPackage[];
}

export interface HostStats {
    /** CpuStats */
    cpu: unknown;
    /** MemoryStats */
    memory: unknown;
    /** StorageStats */
    storage: unknown;
    /** NetworkStats */
    network: unknown;
    /** DiskStats */
    disk: unknown;
    /** u64 */
    timestamp: number;
}

export interface HostsEntry {
    /** string */
    ip: string;
    /** string */
    domain: string;
    /** bool */
    configured: boolean;
    /** bool */
    managed_by_stackvo: boolean;
}

export interface HostsPlan {
    /** string[] */
    add: string[];
    /** string[] */
    remove: string[];
    /** string */
    preview: string;
    /** bool */
    changed: boolean;
    /** string */
    path: string;
}

export interface HtmlCheck {
    /** f64 */
    supported: number;
    /** f64 */
    partial: number;
    /** f64 */
    unsupported: number;
    /** u32 */
    tests: number;
    warnings: Record<string, unknown>;
}

export interface InstancePlan {
    /** string — the id this would take */
    id: string;
    /** string? — why a second instance is not allowed, or null */
    refused?: string;
    /** InstanceSetting[] — the manifest's, with defaults as values and secrets unmasked */
    settings: InstanceSetting[];
    /**
     * DeclaredPort[] — host is what the allocator would choose, or null when it could find nothing
     */
    ports: DeclaredPort[];
}

export interface InstanceRow {
    /** string — the slug of service@version */
    id: string;
    /** string */
    service: string;
    /** string */
    version: string;
    /** boolean */
    enabled: boolean;
    /** boolean — holds the pre-package network alias */
    primary: boolean;
    /** string */
    container: string;
    /** string[] */
    aliases: string[];
    /** Record<handle, number> */
    ports: Record<string, unknown>;
    /** boolean */
    packagePresent: boolean;
}

export interface InstanceSetting {
    /** string — as the manifest spells it, no SERVICE_<ID>_ prefix */
    key: string;
    /** string — the manifest's type: string | secret | int | bool | enum | instanceRef */
    kind: string;
    /** string — masked when secret */
    value: string;
    /** bool */
    secret: boolean;
    /** bool — the value in force is the manifest's own default */
    isDefault: boolean;
    /**
     * string? — the manifest's own default, so the form can put it back; null for a secret, whose default is never handed out unasked
     */
    defaultValue?: string;
    /** bool — instance_apply_settings refuses to empty one of these */
    required: boolean;
    /**
     * string[] — an offer, not a constraint. For an instanceRef it is the installed instances answering the capability it names
     */
    options: string[];
    /** Record<locale, string> */
    label: Record<string, unknown>;
}

export interface LinkCheck {
    /** u32 */
    errors: number;
    links: Record<string, unknown>;
}

export interface LogFile {
    /** string */
    id: string;
    /** string */
    label: string;
    /** 'application'|'server' */
    group: 'application' | 'server';
    /** u64 */
    bytes: number;
    /** i64? (unix seconds) */
    modified?: number;
}

export interface MailAttachment {
    /** string */
    partId: string;
    /** string */
    fileName: string;
    /** string */
    contentType: string;
    /** u64 */
    size: number;
}

export interface MailBody {
    /** string? */
    text?: string;
    /** string? */
    html?: string;
    /** [{ "name": "string", "value": "string" }] */
    headers: Record<string, unknown>;
    /** MailAttachment[] */
    attachments: MailAttachment[];
    /** u64? */
    size?: number;
}

export interface MailMessage {
    /** string */
    id: string;
    /** string */
    from: string;
    /** string[] */
    to: string[];
    /** string[] */
    cc: string[];
    /** string[] */
    bcc: string[];
    /** string[] */
    replyTo: string[];
    /** string */
    subject: string;
    /** string? */
    date?: string;
    /** string? */
    snippet?: string;
    /** bool */
    read: boolean;
}

export interface MailStatus {
    /** bool */
    available: boolean;
    /** 'mailhog'|'mailpit'|null */
    kind: 'mailhog' | 'mailpit' | null;
    /** string? */
    service?: string;
    /** bool */
    enabled: boolean;
    /** bool */
    running: boolean;
    /** string? */
    ui_url?: string;
    /** number */
    total: number;
    /** number? */
    unread?: number;
    /** string? */
    error?: string;
}

export interface MarketPackage {
    /** string */
    service: string;
    /** string */
    category: string;
    /** Record<locale, string> */
    name: Record<string, unknown>;
    /** Record<locale, string> */
    summary: Record<string, unknown>;
    /** string[] */
    capabilities: string[];
    /**
     * string[] — search terms the index publishes, so mysql is findable by typing 'database' and by typing 'mariadb'
     */
    keywords: string[];
    /** boolean — whether two versions may run at once */
    multiple: boolean;
    /** MarketVersion[] */
    versions: MarketVersion[];
}

export interface MarketStatus {
    /** boolean — false before the first refresh */
    fetched: boolean;
    /** number | null */
    sequence: number | null;
    /** string | null */
    generatedAt: string | null;
    /** string | null */
    expires: string | null;
    /** string | null — `local` or `https` */
    sourceKind: string | null;
    /** string | null */
    sourceLocation: string | null;
    /** number */
    packages: number;
    /** number — version directories on this machine */
    installed: number;
    /** boolean — whether signatures are being checked; false until a key is pinned */
    signed: boolean;
    /**
     * boolean — whether policy.market.requireSignature is set. Reported apart from `signed` because the pair is the story: required and not happening is a refusal, and it is the state a managed machine is in until ADR 0015's key exists.
     */
    signatureRequired: boolean;
    /**
     * string | null — the bundle policy.market.offlineBundle points at, which wins over the path the user chose. ADR 0011 makes this the only way an air-gapped machine ever gets a catalogue.
     */
    offlineBundle: string | null;
    /**
     * boolean — whether a policy says anything about the market at all, so the page can explain a refusal before it happens
     */
    constrained: boolean;
}

export interface MarketVersion {
    /** string */
    version: string;
    /** boolean — what `latest` resolves to */
    recommended: boolean;
    /** 'supported' | 'deprecated' | 'eol' */
    support: 'supported' | 'deprecated' | 'eol';
    /** string | null */
    eolDate: string | null;
    /** number | null */
    sizeBytes: number | null;
    /** boolean */
    installed: boolean;
    /** boolean — an instance names it */
    inUse: boolean;
}

export interface Migration {
    /** string (compose file path) */
    source: string;
    /** string? */
    app_service?: string;
    /** 'php'|'node'|null */
    runtime: 'php' | 'node' | null;
    /** string? */
    server?: string;
    /** string? */
    php_version?: string;
    /** string? */
    node_version?: string;
    /** string? */
    document_root?: string;
    /** string? */
    domain?: string;
    /** number? */
    port?: number;
    /** string[] */
    extensions: string[];
    services: Record<string, unknown>;
    /** string[] */
    unmapped: string[];
    /** string[] */
    evidence: string[];
}

export interface MigrationPlan {
    /** Migration */
    migration: Migration;
    /** ProjectSpec */
    spec: ProjectSpec;
    /** PresetPlan */
    env: PresetPlan;
    /** bool */
    already_managed: boolean;
}

export interface MissingPackage {
    /** string */
    service: string;
    /** string */
    version: string;
    /**
     * boolean — whether the cached index offers it, which decides whether the UI can offer a button or only an explanation
     */
    installable: boolean;
}

export interface PhpIniStatus {
    /** bool */
    supported: boolean;
    /** bool */
    exists: boolean;
    /** string */
    path: string;
    /** string */
    container_path: string;
    /** bool? */
    mounted?: boolean;
    /** bool */
    running: boolean;
    /** Record<string, string> */
    values: Record<string, unknown>;
    /** Record<string, string> */
    unmanaged: Record<string, unknown>;
    /** bool */
    needs_recreate: boolean;
    /** string? */
    warning?: string;
    /** Record<string, string>? */
    effective?: Record<string, unknown>;
    /** string */
    overlay_path: string;
}

export interface PolicyMarket {
    /** boolean — whether the policy says anything about the market at all */
    constrained: boolean;
    /** string | null — a mirror the organisation runs; read by the network source (Faz 5) */
    registryUrl: string | null;
    /**
     * string | null — a directory to install from with no network. ADR 0011 makes this the only way an air-gapped machine gets a catalogue, so it is a first-class install path rather than an enterprise extra
     */
    offlineBundle: string | null;
    /**
     * boolean — the one key in the block that is a lock rather than a note: it can only turn verification ON, and there is no value of it that turns a check off (ADR 0009). Raises market::Trust to Signed, which refuses today because no key is pinned
     */
    requireSignature: boolean;
    /** string[] — services that may be installed. EMPTY MEANS NO OPINION, not 'none' */
    allowedPackages: string[];
    /**
     * string[] — registries an image may come from; a reference with no host counts as docker.io. Empty means no opinion
     */
    allowedRegistries: string[];
    /** boolean | null */
    autoUpdate: boolean | null;
    /** number — how many extra signing keys were supplied, not which */
    additionalKeys: number;
}

export interface Port {
    /** u16 */
    container: number;
    /** u16? */
    host?: number;
    /** 'tcp'|'udp' */
    protocol: 'tcp' | 'udp';
}

export interface Preferences {
    /** 'tr'|'en' */
    locale: 'tr' | 'en';
    /** 'light'|'dark'|'system' */
    theme: 'light' | 'dark' | 'system';
    /** string? */
    editor_command?: string;
    /** bool */
    start_minimized: boolean;
    /** bool */
    autostart: boolean;
    /** bool */
    notify_on_build: boolean;
}

export interface Preflight {
    /** 'macos'|'windows'|'linux' */
    os: 'macos' | 'windows' | 'linux';
    /** bool */
    ready: boolean;
    requirements: Record<string, unknown>;
}

export interface Preset {
    /** 'stackvo.preset' */
    kind: 'stackvo.preset';
    /** number */
    version: number;
    /** string? */
    name?: string;
    /** string? */
    description?: string;
    /** Record<string, { enabled: bool, version?: string }> */
    services: Record<string, unknown>;
    /** Record<string, string> */
    settings: Record<string, unknown>;
}

export interface PresetPlan {
    /** string? */
    name?: string;
    /** string? */
    description?: string;
    changes: Record<string, unknown>;
    /** string[] */
    rejected: string[];
    /** number */
    unchanged: number;
    /** bool */
    needs_regenerate: boolean;
}

export interface ProfileFile {
    /** string (cachegrind.out.*) */
    id: string;
    /** number */
    bytes: number;
    /** i64? (unix seconds) */
    modified?: number;
    /** bool */
    compressed: boolean;
}

export interface ProfileReport {
    /** string */
    creator: string;
    /** string */
    cmd: string;
    /** string[] */
    events: string[];
    /** number[] */
    summary: number[];
    /** number */
    self_total: number;
    functions: Record<string, unknown>;
    /** number */
    function_count: number;
    /** bool */
    truncated: boolean;
}

export interface ProfilerStatus {
    /** XdebugStatus */
    xdebug: XdebugStatus;
    /** 'debug'|'profile'|'trace' */
    mode: 'debug' | 'profile' | 'trace';
    /** string (XDEBUG_TRIGGER) */
    trigger: string;
    /** ProfileFile[] */
    profiles: ProfileFile[];
    /** ProfileFile[] — read by profiler_flame, not profiler_read */
    traces: ProfileFile[];
    /** number — profiles AND traces; they share a directory and a disk */
    bytes: number;
    /** string */
    directory: string;
}

export interface ProgressEvent {
    /** string */
    operation_id: string;
    /** string */
    subject: string;
    /** string */
    line: string;
}

export interface Project {
    /** string */
    name: string;
    /** string */
    domain: string;
    /** 'php'|'node' */
    runtime: 'php' | 'node';
    /** string */
    path: string;
    /** string */
    container_name: string;
    /** bool */
    running: boolean;
    /** bool */
    built: boolean;
    /** ProjectManifest */
    manifest: ProjectManifest;
    /** bool */
    manifest_valid: boolean;
    /** string[] */
    manifest_errors: string[];
    /** bool */
    domain_configured: boolean;
    /** bool */
    generated_stale: boolean;
    /** Port[] */
    ports: Port[];
}

export interface ProjectLogFile {
    /** string */
    project: string;
}

/** Defined by `project.schema.json` — this contract only names it. */
export type ProjectManifest = Record<string, unknown>;

export interface ProjectSpec {

}

export interface PruneReport {
    /** u64 */
    imagesDeleted: number;
    /** u64 */
    volumesDeleted: number;
    /** u64 */
    spaceReclaimed: number;
}

export interface PtyTarget {
    oneOf: Record<string, unknown>;
}

export interface QuickCommand {
    /** string */
    id: string;
    /** string */
    display: string;
    /** string */
    about: string;
    /** bool */
    interactive: boolean;
    /** string (the marker file) */
    because: string;
}

export interface ReplRunner {
    /** string */
    id: string;
    /** string (the command, snippet excluded) */
    display: string;
    /** 'php'|'python'|'ruby'|'javascript' */
    language: 'php' | 'python' | 'ruby' | 'javascript';
    /** bool */
    booted: boolean;
    /** string */
    about: string;
    /** string (the marker file) */
    because: string;
}

export interface ReplRun {
    /** string */
    runner: string;
    /** string */
    display: string;
    /** string */
    stdout: string;
    /** string */
    stderr: string;
    /** int? */
    exit_code?: number;
    /** int */
    ms: number;
    /** bool */
    timed_out: boolean;
    /** bool */
    truncated: boolean;
    /** bool */
    limited: boolean;
}

export interface ReplSnippet {
    /** int (unix seconds) */
    at: number;
    /** string */
    runner: string;
    /** string */
    code: string;
}

export interface ReleasePlan {
    /** 'layer'|'retag' */
    strategy: 'layer' | 'retag';
    /** string */
    base_image: string;
    /** string */
    tag: string;
    /** string (empty for retag) */
    dockerfile: string;
    /** [pattern, reason][] */
    excluded: (Record<string, unknown>)[];
    /** string[] */
    warnings: string[];
    /** string */
    app_path: string;
    /** string */
    runtime: string;
}

export interface ReleaseResult {
    /** ReleasePlan */
    plan: ReleasePlan;
    /** { env_files: string[], xdebug_active: bool?, has_app: bool, clean: bool } */
    verification: Record<string, unknown>;
}

export interface Service {
    /** string — the instance id (mysql-8-0) on the market model, the service id before it */
    id: string;
    /** string */
    category: string;
    /** bool — whether it should run, which is not whether it is running */
    enabled: boolean;
    /** bool */
    running: boolean;
    /** bool — a container exists, running or not */
    built: boolean;
    /** string? */
    version?: string;
    /** string */
    containerName: string;
    /** string? — the whole domain, suffix included */
    url?: string;
    /**
     * string? — 'healthy' | 'unhealthy' | 'starting'; null when the image declares no healthcheck
     */
    health?: string;
    /** u16? */
    hostPort?: number;
    /** Port[] */
    ports: Port[];
    /**
     * DeclaredPort[] — every port the package declares, by handle, with the host number in force; empty before the migration
     */
    declaredPorts: DeclaredPort[];
    /**
     * string[] — the Docker network names this answers to, its own first; the second is the pre-package alias
     */
    aliases: string[];
    /** string? — 'supported' | 'deprecated' | 'eol', from the version manifest */
    support?: string;
    /** string? — ISO date upstream support ends or ended */
    eolDate?: string;
    /** CompanionRow[] — containers shipped with this instance and not separately installable */
    companions: CompanionRow[];
    /** Credential[] */
    credentials: Credential[];
    /** DependencyRow[] */
    required: DependencyRow[];
    /** DependencyRow[] */
    optional: DependencyRow[];
    /** string[] — the subject of every required dependency that is unprovided or stopped */
    unmetDependencies: string[];
}

export interface Bundled {
    /** number — services the index lists */
    packages: number;
    /**
     * number — versions whose files were carried; lower than the index's total when something was withdrawn
     */
    versions: number;
    /** number — files written, index and package.json included */
    files: number;
    /** number — their total size, so the person choosing a stick knows */
    bytes: number;
    /**
     * string[] — versions deliberately not carried, each with its reason. Empty is the normal case; a non-empty list is a fact the person carrying the bundle should read before they walk away from the network
     */
    skipped: string[];
    /**
     * boolean — whether registry.json.minisig travelled with it. A machine whose policy sets requireSignature refuses a bundle without one, and this is where that is cheap to find out
     */
    signed: boolean;
}

export interface SourceProbe {
    /** string — exactly what was asked for */
    location: string;
    /**
     * string — where the bytes were actually fetched from; differs when a GitHub repository URL was translated
     */
    resolved: string;
    /** string — `local` or `https` */
    kind: string;
    /** boolean */
    reachable: boolean;
    /** number */
    packages: number;
    /** number */
    versions: number;
    /** number | null */
    sequence: number | null;
    /** string | null */
    generatedAt: string | null;
    /** string | null */
    expires: string | null;
    /** number | null — the cached index's sequence, when there is one */
    currentSequence: number | null;
    /**
     * boolean — this index is older than the one already here, which market_refresh refuses (T-6, replay)
     */
    goesBackwards: boolean;
    /** string | null */
    error: string | null;
    /** string | null */
    hintKey: string | null;
}

export interface StatSample {
    /** u64 */
    t: number;
    /** f32 */
    cpu: number;
    /** f32 */
    memory: number;
}

export interface SystemResources {
    images: { total: number; in_use: number; unused: number; size: number };
    volumes: { total: number; in_use: number; unused: number; size: number };
}

export interface TunnelStatus {
    /** string */
    project: string;
    /** bool */
    running: boolean;
    /** string? */
    url?: string;
    /** string */
    container: string;
}

export interface UpdateInfo {
    /** bool */
    available: boolean;
    /** string? */
    version?: string;
    /** string? */
    notes?: string;
}

export interface ValidationReport {
    /** bool */
    valid: boolean;
    errors: Record<string, unknown>;
    warnings: Record<string, unknown>;
}

export interface WorkerStatus {
    /** string */
    project: string;
    /** 'queue'|'scheduler'|'horizon' */
    kind: 'queue' | 'scheduler' | 'horizon';
    /** bool */
    running: boolean;
    /** i64? */
    restarts?: number;
    /** string */
    container: string;
}

export interface Workspace {
    /** string? */
    root?: string;
    /** bool */
    valid: boolean;
    /** 'stored'|'env'|'discovered'|'none' */
    source: 'stored' | 'env' | 'discovered' | 'none';
    /** string? */
    stackvo_version?: string;
    /** string? */
    projects_dir?: string;
    /** string? */
    env_file?: string;
}

export interface XdebugStatus {
    /** bool */
    supported: boolean;
    /** bool */
    enabled: boolean;
    /** bool? */
    active?: boolean;
    /** bool */
    needs_rebuild: boolean;
    /** bool */
    running: boolean;
    /** number */
    port: number;
    /** string */
    mode: string;
    /** string */
    ide_key: string;
    /** string? */
    server_name?: string;
    /** string? */
    host_path?: string;
    /** string */
    container_path: string;
    /** string? */
    php_version?: string;
    /** string? */
    pecl_version?: string;
    /** string */
    overlay_path: string;
}

export interface LocalOverride {
    /** string */
    text: string;
    /** bool */
    exists: boolean;
    /** string[] */
    applied: string[];
    /** string[] */
    refused: string[];
    /** bool? */
    ignored?: boolean;
}

export interface HookPlan {
    /** string — post-build | post-start | pre-stop */
    event: string;
    /** HookStep[] */
    steps: HookStep[];
    /**
     * string? — what an approval would be recorded against; absent when the project has no host steps
     */
    digest?: string;
}

export interface HookStep {
    /** string — exec | host */
    kind: string;
    /** string — display only; nothing parses it back */
    command: string;
    /** string? — policy-off | policy-host | needs-consent; absent means it runs */
    blocked?: string;
}

export interface PackageReport {
    /** string */
    service: string;
    /** string */
    version: string;
    /** string */
    dir: string;
    /** string[] — files whose hash the manifest had wrong */
    resealed: string[];
    /** string[] — everything sealing cannot fix */
    problems: string[];
}

export interface DnsStatus {
    /** string — resolver | network-manager | dnsmasq | systemd-resolved | nrpt | manual */
    mechanism: string;
    /** bool — whether this app can apply the change itself, or only report it */
    writable: boolean;
    /** string */
    suffix: string;
    /** string — the last label, which is what a resolver is pointed at */
    tld: string;
    /** number */
    port: number;
    /** bool — UDP */
    listening: boolean;
    /** bool — the second socket, reported separately because a port can be half-taken */
    tcp: boolean;
    /** string? — the file this mechanism writes, where it has one */
    file?: string;
    /** bool — whether this machine currently asks us for the suffix */
    configured: boolean;
    /** string — the file this app would write, or the line the user must place */
    instruction: string;
    /** string? — what is reloaded after the write, spelled out rather than done quietly */
    reload?: string;
    /** string? — a file already at that path that is not ours, and what it says */
    foreign?: string;
    /** string[] — resolver files this app wrote for a suffix the workspace has since left */
    stale: string[];
    /**
     * bool — the machine asks us and nothing answers, which is the state where every name under the suffix fails
     */
    broken: boolean;
}

export interface DnsCheck {
    /** string — the probe name, under the suffix and in no hosts file */
    name: string;
    /** DnsProbe — asked the responder directly */
    udp: DnsProbe;
    /** DnsProbe — asked the responder over TCP */
    tcp: DnsProbe;
    /** DnsProbe — asked this machine, the way a browser would */
    system: DnsProbe;
    /** DnsProbe — whether the rest of the internet still resolves */
    public: DnsProbe;
    /** bool — udp and system together, which is the pair that means 'this works' */
    ok: boolean;
}

export interface DnsProbe {
    /** bool */
    ok: boolean;
    /** string — what came back, in words, for a screen that has to explain a failure */
    detail: string;
}

export interface UserRoute {
    /** string */
    domain: string;
    /** string — http:// or https://, origin only */
    target: string;
    /** bool */
    enabled: boolean;
    /** string? — what the user typed, when normalisation changed it */
    rewrittenFrom?: string;
    /** string[] — true and not errors */
    notes: string[];
    /** string? — set instead of `notes` when the route no longer normalises */
    error?: string;
}

export interface DbMovePlan {
    /** string — instance id */
    from: string;
    /** string — instance id */
    to: string;
    /** string */
    fromService: string;
    /** string */
    toService: string;
    /** string */
    fromVersion: string;
    /** string */
    toVersion: string;
    /** bool */
    possible: boolean;
    /** string? — why not */
    refused?: string;
    /** string[] — true and not blocking */
    warnings: string[];
}

export interface DbMoved {
    /** string */
    from: string;
    /** string */
    to: string;
    /**
     * number — the size of the dump that crossed, the only number that says anything actually moved
     */
    bytes: number;
}

export interface DbInstance {
    /** string */
    id: string;
    /** string */
    service: string;
    /** string */
    version: string;
    /** string */
    kind: string;
    /** string */
    container: string;
    /** bool */
    enabled: boolean;
    /** bool */
    running: boolean;
}

export interface PushPlan {
    /** string */
    tag: string;
    /** string? — the host the tag names */
    registry?: string;
    /** bool */
    possible: boolean;
    /** string? — why not */
    refused?: string;
    /**
     * bool? — whether this registry is in the user's Docker config; null means the answer could not be read, which is NOT “not logged in”
     */
    authenticated?: boolean;
    /** string[] */
    warnings: string[];
}

export interface IdleProject {
    /** string */
    project: string;
    /** string */
    router: string;
    /** number? — since the last request; absent when the log has never mentioned this router */
    seconds?: number;
    /** bool */
    suspendable: boolean;
}

export interface PerfLayer {
    /** string — project-relative directory */
    path: string;
    /** bool — whether the setting says it lives in a volume */
    enabled: boolean;
    /** string — the named volume, stackvo-cache-<project>--<path> */
    volume: string;
    /** bool — whether that volume exists on the engine */
    exists: boolean;
    /** number? — what it holds, when the engine can say */
    bytes?: number;
    /** bool — whether the host still has a copy, which is what an editor indexes */
    onHost: boolean;
    /** number? — files in the host copy, counted up to a cap */
    hostFiles?: number;
}

export interface Install {
    /** 'xampp' | 'laragon' | 'mamp' | 'valet' | 'sail' */
    source: 'xampp' | 'laragon' | 'mamp' | 'valet' | 'sail';
    /** string — the installation root, or for Valet/Sail the directory that was pointed at */
    path: string;
    /**
     * Array<{ name: string, path: string, domain: string | null, bytes: number, partial: boolean, detected: Detected, taken: boolean, services?: string[] }>
     */
    sites: Record<string, unknown>[];
}

export interface SiteSettings {
    /**
     * Record<string,string> - set on the project's container, never written into the application's own .env
     */
    env: Record<string, unknown>;
    /** bool - autoindex for nginx, file_server browse for Caddy */
    directoryListing: boolean;
    /** bool - forward the host's SSH agent into the container */
    sshAgent: boolean;
    /** bool - false for Apache and Swoole, which have no configuration file */
    listingSupported: boolean;
    /** bool - false when no agent is running on this machine */
    agentAvailable: boolean;
    /** string - named so the pane can say which server cannot list a directory */
    server: string;
}

export interface Worktree {
    /**
     * string — the project name, which is also the directory name, and the identity everything else keys off
     */
    name: string;
    /** string — the project it was branched from */
    parent: string;
    /** string — the git branch checked out into it */
    branch: string;
    /**
     * string — a subdomain of the parent's, so it stays inside any wildcard route or certificate the parent has
     */
    domain: string;
    /**
     * string — absolute, and recorded rather than derived: the project tree can be moved, and a derived path would send `git worktree remove` somewhere git has never heard of
     */
    path: string;
    /** { instance: string, name: string, seededFrom?: string } | null */
    database: Record<string, unknown> | null;
    /**
     * Record<string,string> — what somebody typed. The database credentials are NOT here; they are derived from the instance on every render
     */
    env: Record<string, unknown>;
    /** string — RFC 3339 */
    createdAt: string;
}

export interface WorktreeRow extends Worktree {
    /** bool — is the directory still there */
    exists: boolean;
    /**
     * bool | null — uncommitted work a removal would discard. null when git could not answer, which is a third state and not a warning
     */
    dirty: boolean | null;
    /**
     * bool — the record points somewhere git no longer has a checkout, usually because the folder was deleted by hand
     */
    orphaned: boolean;
}

export interface WorktreePlan {
    /** string */
    parent: string;
    /** string */
    branch: string;
    /** bool — whether the branch would be created rather than checked out */
    newBranch: boolean;
    /** string */
    name: string;
    /** string */
    path: string;
    /** string */
    domain: string;
    /** { instance: string, service: string, name: string, seed: bool, source?: string } | null */
    database: Record<string, unknown> | null;
    /** string[] — what proceeds anyway */
    warnings: string[];
    /** string | null — one sentence naming why it cannot */
    refused: string | null;
    /** bool */
    possible: boolean;
}

export interface WorktreeSupport {
    /** bool */
    gitAvailable: boolean;
    /** bool — is the project's directory a git repository at all */
    repository: boolean;
    /**
     * bool — is it itself a linked worktree, as git sees it. Different from `record`, which means this app made it
     */
    linked: boolean;
    /** Worktree | null */
    record: Worktree | null;
    /**
     * Record<string,string> | null — what this worktree's container is actually given, present only when `record` is. DB_PASSWORD is masked, and so is the same password inside DATABASE_URL
     */
    effectiveEnv: Record<string, unknown> | null;
    /** string | null — what a new worktree's hostname would be built under */
    domain: string | null;
    /** string | null — null for a detached head */
    currentBranch: string | null;
    /** Array<{ name: string, checkedOut: bool, current: bool }> — newest commit first */
    branches: Record<string, unknown>[];
    /** DbInstance[] — stopped ones included and marked */
    instances: DbInstance[];
    /** string | null — why worktrees are unavailable here */
    reason: string | null;
    /** WorktreeRow[] — this project's own */
    worktrees: WorktreeRow[];
}

export interface StackvoApi {
  /**
   * The web UI never needed this — it ran inside the StackVo repo, so its root was `/app` by mount. A desktop app has to be told, or work it out. Returns the resolved root plus how it was resolved, so the UI can show the user which checkout it is driving.
   */
  workspaceGet(): Promise<Workspace>;
  workspaceSet(path: string): Promise<Workspace>;
  bootstrapComplete(): Promise<void>;
  /**
   * There was NO endpoint for this. The web UI could not report a stopped Docker daemon because the UI itself was a container that could not start. This is the single most important new command — it is what makes the desktop app usable when Docker is down.
   */
  engineStatus(): Promise<EngineStatus>;
  /**
   * Everything that has to be true before the app can do anything: a checkout, a reachable daemon, compose v2, the shared network, a projects directory, bash, and mkcert. Without it the app opened regardless and each button failed on its own terms — three different errors, one cause each, none stated up front.
   */
  preflight(): Promise<Preflight>;
  /**
   * A requirement that reports a problem the app could have fixed itself is a diagnosis pretending to be a product. `fixable` on each Requirement says which ids this accepts.
   */
  preflightFix(id: string): Promise<void>;
  /**
   * Launch Docker Desktop (macOS/Windows) or `systemctl --user start docker` (Linux). Resolves the chicken-and-egg problem end to end.
   */
  engineStart(): Promise<void>;
  /**
   * The boot gate answers "can the app run"; this answers the failures that arrive later, one failed compose up at a time. The single most common one — a host port already taken — compose reports as "address already in use" with no word on by what. The doctor names the culprit: the stack's own container (fine), someone else's container (named), or a host process (named, with pid).
   */
  doctor(): Promise<Doctor>;
  /**
   * SystemResources has reported reclaimable bytes since Phase 1 with no way to act on the number. Dangling images accumulate on every rebuild, and this app rebuilds a lot — every Xdebug toggle, every extension change.
   */
  dockerPrune(images: boolean, volumes: boolean, buildCache: 'keep' | 'dangling' | 'all'): Promise<PruneReport>;
  hostStats(): Promise<HostStats>;
  dockerSystemResources(): Promise<SystemResources>;
  /**
   * docker_system_resources reports totals; the question a full disk raises is *which project*. Every StackVo container with its image size and writable layer, plus stack-built images whose container is gone — the bytes nobody remembers spending. No native-binary competitor can ship this at all.
   */
  dockerDiskUsage(): Promise<DiskOwner[]>;
  projectsList(): Promise<Project[]>;
  servicesList(): Promise<Service[]>;
  catalogGet(): Promise<Catalog>;
  serverConfigGet(server: string): Promise<string>;
  serverConfigSet(server: string, content: string): Promise<void>;
  templatesList(): Promise<Record<string, unknown>[]>;
  templateOverride(path: string): Promise<string>;
  templateRevert(path: string): Promise<void>;
  envGet(): Promise<Record<string, unknown>>;
  envDefaults(): Promise<Record<string, unknown>>;
  projectStart(name: string): Promise<void>;
  projectStop(name: string): Promise<void>;
  projectRestart(name: string): Promise<void>;
  projectBuild(name: string, no_cache: boolean): Promise<OperationId>;
  /**
   * The value behind one masked credential on the services list, whichever place this workspace keeps it.
   */
  serviceReveal(service: string, key: string): Promise<string>;
  /**
   * A machine that has never fetched a catalogue has none at all — ADR 0011 embeds nothing — so 'not fetched yet' and 'the catalogue is empty' are different sentences and the market page shows a different thing for each.
   */
  marketStatus(): Promise<MarketStatus>;
  /**
   * Read a catalogue from a directory the user chose — an offline bundle, or a checkout of the packages repository. The only source this build has; HTTPS waits on the key ceremony.
   */
  marketRefresh(location: string): Promise<MarketStatus>;
  /**
   * C-1. Writes a package that is valid on the first read — identity, manifest, fragment, hashes already correct. MUST build the path from the workspace root and checked components: the webview names a service and a version, never a directory. MUST refuse to overwrite an existing version, and MUST leave an existing package.json alone so adding 8.4 does not rewrite the identity somebody filled in for 8.0.
   */
  packageScaffold(category: string, service: string, version: string, image: string): Promise<PackageReport>;
  /** What sealing would change and what would still be wrong afterwards, with nothing written. */
  packageLint(category: string, service: string, version: string): Promise<PackageReport>;
  /**
   * Recomputes the hashes after an edit and THEN validates — parse, manifest check, compose policy — refusing the whole operation if any fail. Writing the hashes of a fragment the policy rejects would be a tool for producing packages that install and cannot run.
   */
  packageSeal(category: string, service: string, version: string): Promise<PackageReport>;
  /**
   * What is published, and for each version whether it is already installed and whether an instance is using it — so the UI can offer Install or refuse Uninstall without a second round trip.
   */
  marketCatalog(): Promise<MarketPackage[]>;
  /** Fetch one package, verify it whole, and put it where the renderer looks. */
  marketInstall(service: string, version: string): Promise<MarketStatus>;
  /**
   * Remove a package's files. Data is not touched — ADR 0012 puts that behind purgeData on a command that also stops containers.
   */
  marketUninstall(service: string, version: string): Promise<MarketStatus>;
  /**
   * Answering 'is my address right' without doing the thing. The first address anybody pastes is the repository's web page, which a working server correctly 404s, and there was no way to find that out except by refreshing and reading the failure.
   */
  marketProbe(location: string): Promise<SourceProbe>;
  /**
   * Write everything an air-gapped machine needs into one directory, on a machine that still has a network. ADR 0011 makes market.offlineBundle the only way a catalogue reaches a machine with no network, and until now nothing could produce one — the consuming half shipped without the producing half (§3 #31).
   */
  marketBundle(destination: string): Promise<Bundled>;
  /**
   * The one migration that touches a workspace somebody is already using. handover.rs is built plan-then-apply so the reasons survive; a UI with only the apply half would have thrown them away.
   */
  handoverPreview(): Promise<HandoverPreview>;
  /** Write the instance table from `.env`, once. */
  handoverApply(): Promise<HandoverPreview>;
  /** What is installed, how many times, and which one answers to the pre-package name. */
  instanceList(): Promise<InstanceRow[]>;
  /**
   * What creating this instance would produce, before it does — so a first-boot password can be chosen at the one moment an image will read it.
   */
  instancePlan(service: string, version: string): Promise<InstancePlan>;
  /**
   * Create an instance of an installed package. Returns its id — the slug of service@version, which every derived name comes from.
   */
  instanceCreate(service: string, version: string, settings?: Record<string, unknown>, ports?: Record<string, unknown>): Promise<string>;
  /** Forget an instance. Volumes are left alone (ADR 0012). */
  instanceRemove(id: string): Promise<void>;
  /** Move the pre-package name — `stackvo-mysql` — to another version of the same service. */
  instancePromote(id: string): Promise<void>;
  /**
   * Write the instance down as on, regenerate, then bring its compose profile up. The order is the one service_enable uses: the compose file has to describe the container before compose is asked to start it.
   */
  instanceEnable(id: string): Promise<OperationId>;
  /** Switch an instance off. NOTHING IS DELETED (ADR 0012) — not the volume, not the image. */
  instanceDisable(id: string): Promise<OperationId>;
  /**
   * The container, without touching what the instance table says. Distinct from enable/disable, which change whether the instance is part of the stack at all.
   */
  instanceStart(id: string): Promise<void>;
  /**
   * The container, without touching what the instance table says. Distinct from enable/disable, which change whether the instance is part of the stack at all.
   */
  instanceStop(id: string): Promise<void>;
  /**
   * The container, without touching what the instance table says. Distinct from enable/disable, which change whether the instance is part of the stack at all.
   */
  instanceRestart(id: string): Promise<void>;
  /**
   * What one instance is configured with, as its manifest declares it. Replaces service_settings, which read SERVICE_<ID>_* keys out of .env — a name that identifies a service, of which two versions can be running.
   */
  instanceSettings(id: string): Promise<InstanceSetting[]>;
  /** The real value behind a masked secret, for the one instance and the one key. */
  instanceReveal(id: string, key: string): Promise<string>;
  /**
   * Write an instance's settings and rebuild its container with them. Replaces service_apply_settings, which wrote them to .env.
   */
  instanceApplySettings(id: string, patch: Record<string, unknown>, ports?: Record<string, unknown>): Promise<OperationId>;
  containerInspect(name: string): Promise<unknown>;
  containerStats(name: string): Promise<ContainerStats>;
  /**
   * Log following existed only as `stackvo logs` in the CLI. The UI had no way to tail a container.
   */
  containerLogsOpen(name: string, tail: number, follow: boolean): Promise<unknown>;
  containerLogsClose(stream_id: string): Promise<void>;
  /**
   * The container stream carries stdout and stderr, which is what the entrypoint and the web server say — not what the application records. A Laravel exception goes to storage/logs/laravel.log, an nginx 502 to the mounted error.log, and a queue worker that died to its own file under supervisord. None of those reach stdout, so none of them were visible anywhere in the app.
   */
  appLogs(name: string): Promise<LogFile[]>;
  /**
   * Follows one of those files. Shares the event pair with container_logs_open because the viewer renders one kind of line; a second pair would have meant a second frontend listener free to drift from the first. Closed with container_logs_close — one registry of abort handles, one way to stop a stream.
   */
  appLogOpen(name: string, id: string, tail_bytes?: number): Promise<unknown>;
  /**
   * The per-project viewer answers "what did this project write". It cannot answer "which of my eight projects just errored", which is the question you have when you do not yet know where to look. Herd sells the cross-project view as a Pro feature.
   */
  appLogsAll(): Promise<ProjectLogFile[]>;
  /**
   * One live tail across every project, so an error surfaces without first guessing which project to open.
   * Returns the first scan's coverage rather than emitting it. An event would race the caller — the spawned task can emit before the frontend holds the stream id it filters events by, and the coverage line would then stay blank until the next rediscovery thirty seconds later. logs:sources carries updates only, because only an update has somewhere to arrive.
   */
  appLogsAllOpen(projects?: string[]): Promise<FanoutStream>;
  /**
   * Settings.vue is a 24-line stub because .env was read-only over HTTP. Writing MUST preserve comments, key order and blank lines — a naive rewrite destroys a 159-key hand-annotated file.
   */
  envSet(patch: Record<string, unknown>): Promise<void>;
  /**
   * `stackvo generate` was only ever invoked as a side effect of project_create / service_enable, via execAsync on a shell string. Making it a first-class command is what lets the fs-watcher regenerate automatically.
   */
  generateRun(scope: 'all' | 'projects' | 'services'): Promise<OperationId>;
  composeUp(mode: 'minimal' | 'services' | 'projects' | 'all' | 'custom', profiles: string[]): Promise<OperationId>;
  /**
   * `stackvo down` existed only in the CLI — the web UI could never stop the stack, because doing so would have killed the UI itself.
   */
  composeDown(): Promise<OperationId>;
  /** G-4. A move names two instances, and `mysql-8-0` and `mysql-8-4` both answer to `mysql`. */
  dbInstances(): Promise<DbInstance[]>;
  /**
   * I-2. Reports the idle time even with the threshold off — the number is worth seeing before somebody chooses what to set it to.
   */
  projectsIdle(): Promise<IdleProject[]>;
  /**
   * Returns NAMES, not a count: this is a background action whose whole risk is being surprising, and “3 suspended” is a number somebody then has to match against a list. MUST skip a project something else holds rather than queueing behind it — a sweep that waited behind a build would stop the container the moment the build finished.
   */
  projectsSuspendIdle(): Promise<string[]>;
  /**
   * G-4. Plan-then-apply, the same shape as hosts_plan/hosts_apply, because the target is EMPTIED and that sentence has to be on screen before anybody presses anything. MUST touch no container.
   */
  dbMovePlan(from: string, to: string): Promise<DbMovePlan>;
  /**
   * MUST re-check the plan rather than trusting the caller's — the plan crossed the IPC boundary and came back. MUST hold BOTH instances busy: a move reads one database and replaces another, and either being started or dumped underneath it is a torn result nobody could explain. Goes through a file rather than a pipe, and the file is KEPT on failure with its path in the error — at that moment it is the only copy of the source outside a container and the target is already replaced.
   */
  dbMoveApply(from: string, to: string): Promise<DbMoved>;
  /**
   * E-4. A route the renderer skips MUST still appear here, or the screen shows a route the proxy does not have.
   */
  routesList(): Promise<UserRoute[]>;
  /**
   * Replaces the whole list and regenerates. Whole-list rather than add/remove/edit: the file is a handful of pairs in one table, and three commands over one small document is three chances for the file and the screen to disagree about order. MUST check every route before writing any, so one bad row fails the save rather than writing half of it, and MUST refuse a duplicate domain — two routers on one name is a coin toss the user cannot see.
   */
  routesSave(routes: UserRoute[]): Promise<UserRoute[]>;
  /**
   * E-1. /etc/hosts needs a password per project and cannot express a wildcard at all, which is why E-2 was left half-done.
   */
  dnsStatus(): Promise<DnsStatus>;
  /**
   * Binds 127.0.0.1 on a high port — 53 needs root at every start and does not need to, because macOS resolver files take a `port` directive. MUST be idempotent for the same suffix and MUST restart for a different one: a responder left serving the old TLD answers for names nothing renders and refuses the ones it does.
   */
  dnsStart(): Promise<DnsStatus>;
  /** Stops and waits for the thread, so the socket is actually free afterwards. */
  dnsStop(): Promise<DnsStatus>;
  /**
   * Writes /etc/resolver/<tld> with a password. Deliberately separate from dns_start, the same separation hosts_plan/hosts_apply has: one is a socket this app owns, the other changes how the whole machine resolves names, and folding them into one button would make a password prompt appear from something that reads like turning on a feature.
   */
  dnsResolverInstall(): Promise<DnsStatus>;
  /**
   * Turning it off has to be as easy as turning it on. MUST undo the reload as well as the file — a drop-in removed from a dnsmasq nobody told is a machine still asking a port that may be gone.
   */
  dnsResolverRemove(): Promise<DnsStatus>;
  /**
   * dns_status reads files and sockets this app owns, which cannot answer the only question a user has: does THIS MACHINE resolve a name under the suffix. Reading back a resolver file proves a write happened. So this asks the responder over UDP and TCP, asks the machine through getaddrinfo, and asks whether public names still resolve — four answers because the repair for each is different.
   */
  dnsCheck(): Promise<DnsCheck>;
  /**
   * Computes the exact replacement text WITHOUT elevating, so the UI can show a diff and ask first. Elevating and explaining afterwards would be the wrong order for the one operation that touches a system file.
   */
  hostsPlan(add: string[], remove: string[]): Promise<HostsPlan>;
  /**
   * Removes the manual `sudo tee -a /etc/hosts` step from the README. MUST show a diff and require explicit confirmation before elevating; MUST back up the file; MUST only touch lines inside a StackVo-managed marker block.
   */
  hostsApply(add: string[], remove: string[]): Promise<HostsPlan>;
  /**
   * Every project domain with no hosts entry. Drives the one-click fix; the web UI could detect this but had no way to act on it.
   */
  hostsMissing(): Promise<string[]>;
  hostsMissingCore(): Promise<string[]>;
  hostsOverview(): Promise<unknown>;
  /**
   * StackVo has shipped a mail catcher all along and never showed it — reading a captured message meant leaving for a browser tab, which is exactly the round trip four competitors charge for removing.
   */
  mailStatus(): Promise<MailStatus>;
  mailMessages(limit: number): Promise<MailMessage[]>;
  mailMessage(id: string): Promise<MailBody>;
  mailClear(): Promise<void>;
  /**
   * M-2. The catcher catches everything, which is right, and it is not the whole job: the message somebody actually needs to check is the one that renders differently in Outlook, or the invoice a colleague has to look at — both of which mean THIS ONE MESSAGE, TO A REAL ADDRESS. Every rival calls that release and StackVo could not do it. Deliberately not the other shape: pointing the application at a real SMTP server would send the forty password resets a test suite generates in an hour to whatever addresses the fixtures happen to contain.
   */
  mailRelayGet(): Promise<Record<string, unknown>>;
  /** Somewhere to put the relay, once somebody has been told a release was refused. */
  mailRelaySet(config: unknown, password: string | null): Promise<unknown>;
  /** The whole point of M-2: one caught message, sent on to a real person. */
  mailRelease(id: string, to: string[]): Promise<void>;
  mailDelete(id: string): Promise<void>;
  /**
   * A catcher with fifty captured mails needs a search box more than it needs another button. The query reaches the catcher verbatim, so Mailpit's own syntax (from:, to:, subject:, quoted phrases) works as documented rather than being reimplemented worse here.
   */
  mailSearch(query: string, limit?: number): Promise<MailMessage[]>;
  /**
   * The report a developer actually opens a catcher for: how this markup fares across 186 real mail-client features. Mailpit only — null on MailHog, which is how the UI knows to hide the tab instead of showing zeroes.
   */
  mailHtmlCheck(id: string): Promise<HtmlCheck>;
  /** Dead links in a transactional mail are found by the recipient today. */
  mailLinkCheck(id: string): Promise<LinkCheck>;
  /** Reading a captured invoice PDF meant leaving for the browser UI. */
  mailAttachmentSave(id: string, partId: string, path: string): Promise<number>;
  /**
   * Which database services can be dumped, what they are called, and whether they are running. The credentials were already read out of .env and rendered in the services list; what was missing was anything to do with them.
   */
  dbTargets(): Promise<DbTarget[]>;
  /**
   * Lerd sells snapshots, Laragon sells automatic backups, ServBay sells both; StackVo had no way to take a copy of a database at all. The tools are already inside the container — `mysqldump`, `pg_dump` and `mongodump` ship with the images the stack runs.
   */
  dbDump(service: string, path: string): Promise<OperationId>;
  /** A backup nobody has restored is a file, not a backup. */
  dbRestore(service: string, path: string): Promise<OperationId>;
  /**
   * Competitive review G-2: `ddev snapshot` and `lerd db:snapshot` name a point in time and restore it by that name. db_dump could already write a file to a path chosen in a save dialog, which is raw material — a dump in Downloads is not something anybody comes back to.
   */
  dbSnapshots(): Promise<Record<string, unknown>[]>;
  /**
   * Takes a named copy, into a path the app owns. Returns the operation id; progress and completion arrive on the same db:* events db_dump uses.
   */
  dbSnapshotTake(service: string, name: string): Promise<string>;
  /** Puts a named snapshot back, replacing what is in the database. */
  dbSnapshotRestore(service: string, name: string): Promise<string>;
  /** Removes one copy. The way out of a directory that would otherwise only grow. */
  dbSnapshotDelete(service: string, name: string): Promise<void>;
  /**
   * F-2, whose note read 'dump/mail/log three separate screens, no correlation'. What the code thought it had (dd()) and what it actually asked the database for are two halves of one question, and reading them meant comparing clocks by eye across two panes.
   */
  requestTimeline(project: string, service?: string): Promise<unknown>;
  /**
   * F-3. `profiler_read` answers where the time went — a table of the costliest functions — and cannot answer what called that, which is the question a flame view exists for. The parser was already reading caller→callee edges to attribute inclusive cost and was discarding the caller.
   */
  profilerTree(name: string, id: string): Promise<unknown>;
  /**
   * F-3, and the reason the word flame graph can now be used. profiler_tree draws what cachegrind holds — the SUMMED cost of "A called B" over every place A called B — so a function reached from two callers is one box carrying both, and no arrangement of those edges recovers which caller was expensive. An Xdebug trace holds every entry and exit with its depth, which is a stack: folding those gives one box per distinct path, with its own measured width. MUST be a separate command from profiler_tree rather than a flag on it, because the two are different claims about the same picture and a reader cannot tell them apart by looking.
   */
  profilerFlame(name: string, id: string): Promise<Flame>;
  /**
   * M-5, M-6 and M-10 in one document, because they are one kind of thing: a per-project setting the generator cannot read from the manifest (project.schema.json is additionalProperties:false and frozen), kept in .stackvo/site.json so it travels with the project. listingSupported and agentAvailable are answered by the machine rather than assumed by the screen: Apache and Swoole have no configuration file for a directory index, and an agent cannot be forwarded when none is running - a control drawn for either would do nothing.
   */
  siteSettings(name: string): Promise<Record<string, unknown>>;
  /**
   * Whole document rather than per key: three settings in one small file, and three commands over one document is three chances for it and the screen to disagree - the same reasoning routes_save gives. MUST refuse a key that is not a POSIX variable name and a value carrying a line break: the overlay is YAML, where a newline in a scalar ends it and everything after is read as configuration somebody else wrote. Regenerates, because the directory listing lands in a GENERATED server config while the variables and the agent land in a compose overlay - two different paths for one save.
   */
  siteSave(name: string, env: Record<string, unknown>, directoryListing: boolean, sshAgent: boolean): Promise<unknown>;
  /**
   * I-1. A bind mount costs 2-3x a named volume on metadata and writes, which is the measured reason a Docker workflow feels slow on macOS and Windows. This lists the directories worth moving off the host filesystem for one project, whether each is currently in a volume, how big that volume is, and whether the host still has a copy for an editor to index. Suggestions describe the project in front of them — vendor for PHP, storage/framework and bootstrap/cache when it is a Laravel, node_modules when there is a package.json — and are never applied on their own.
   */
  perfStatus(name: string): Promise<PerfLayer[]>;
  /**
   * MUST seed the volume from the host copy BEFORE writing the setting, and MUST fail without writing it if that copy fails. A fresh named volume is empty — Docker seeds one from the image only where the image has content there, and no PHP image ships a vendor/ — so a setting saved first and a copy that failed afterwards is a site that 500s on the next request from a switch that reported success. Turning it OFF copies nothing and deletes nothing: the volume may hold the only copy of a vendor/ that took ten minutes to build. MUST refuse a path that is not relative to the project — the value becomes a container path AND part of a volume name, and it arrives from a JSON file that may have come with a git clone.
   */
  perfSet(name: string, path: string, enabled: boolean): Promise<PerfLayer[]>;
  /**
   * The price of the feature, paid explicitly: the container reads its dependencies from a volume an editor cannot see, so autocomplete needs a copy on the host. It is a SNAPSHOT and the screen says so — the container keeps writing to the volume and this copy does not follow. Replaces the host directory rather than merging into it: a half-updated vendor/ is worse for an index than an old one, because nothing about it says which half is which.
   */
  perfExport(name: string, path: string): Promise<Record<string, unknown>>;
  /**
   * Deletes the volume. Its own act and never part of turning the layer off — a checkbox that throws away thirty thousand files as a side effect is one nobody should trust.
   */
  perfForget(name: string, path: string): Promise<PerfLayer[]>;
  /**
   * F-1, and §2 called it the largest product gap: three competitors sell query logging and N+1 detection, and this stack could not say what the database had been asked. The row also said it needed a collector inside the container — for MySQL and MariaDB that is wrong, and this is why.
   */
  queryLog(service: string): Promise<unknown>;
  /**
   * The log records every statement unsampled and costs write throughput, so it is an instrument you switch on, look at, and switch off — never a default.
   */
  queryLogRecord(service: string, recording: boolean): Promise<unknown>;
  /**
   * Start again from here — what somebody presses before reloading the page they are investigating. Without it every read is the whole session and the one request under study is buried.
   */
  queryLogClear(service: string): Promise<unknown>;
  /**
   * The services sheet showed a container name, a port table and a credentials block, and left the reader to assemble a URI. The obvious assembly — `mongodb://stackvo-mongo:27017/` — cannot work from the host, because that name only resolves on the Docker network. Null for a service nobody connects to with a string.
   */
  serviceConnection(service: string, reveal: boolean): Promise<Connection>;
  /**
   * The picker beside `service_open_in_client` has to be built from what is on this machine, not from a list of clients somebody might have. Empty for most services, which is the answer rather than a failure: a service with no connection string has nothing to open, and an AMQP or SMTP address is not one a desktop database client takes.
   */
  serviceDbClients(service: string): Promise<unknown>;
  /**
   * G-3. The correct connection string has existed since connect.rs was written and the sheet has offered to copy it since; what was missing was the step that pastes it, which everybody was doing by hand.
   */
  serviceOpenInClient(service: string, client: string): Promise<void>;
  /**
   * Step debugging is the thing Docker makes hardest and the thing a PHP developer needs most. `xdebug` was already in the extension catalog with per-version pecl pins, so it could be compiled in by hand-editing stackvo.json — and then it still would not connect, because nothing set xdebug.mode or told it which host the IDE is on. This reports all three layers separately: listed in the manifest, present in the built image, and live in the running container.
   */
  xdebugStatus(name: string): Promise<XdebugStatus>;
  /**
   * One switch instead of three manual steps: adding the extension to the manifest, configuring the client host and port, and telling the IDE where the code is mounted.
   */
  xdebugSet(name: string, enabled: boolean): Promise<XdebugStatus>;
  /**
   * Every competitor exposes memory_limit and upload_max_filesize; StackVo could not, because .stackvo/php.ini was documented but never real — docs/*/configuration/project.md lists it and the old web UI's DockerService.js:388 lists it, but `php.ini` appears NOWHERE in core/cli. No generator mounted it, so dropping the file in did nothing. The mount had to exist before a form was worth building.
   */
  phpIniStatus(name: string): Promise<PhpIniStatus>;
  /**
   * The half of P1-9 that was cut on evidence: `memory_limit` and `upload_max_filesize` are not manifest keys and cannot become them (the schema is additionalProperties: false), and the file the docs pointed at was mounted by nothing.
   */
  phpIniSet(name: string, patch: Record<string, unknown>): Promise<PhpIniStatus>;
  /**
   * The doctor's design rule is a repair next to every finding — hosts get the reviewed diff, stale output gets regenerate, disk gets prune. The extension check shipped without one, which left the panel naming a problem and offering nothing.
   */
  doctorDropExtension(subject: string, extension: string): Promise<Doctor>;
  /**
   * Catching dumps used to cost a container recreate, because the mechanism was two environment variables and a container's environment is fixed at creation. Every debugging session started with a wait.
   */
  debugBridgeSet(name: string, enabled: boolean): Promise<void>;
  debugBridgeEvents(name: string, since?: number): Promise<Record<string, unknown>>;
  debugBridgeClear(name: string): Promise<void>;
  /**
   * The per-project pane cannot answer "which of my eight projects just dumped something" — the question you have before you know which project to open. It is the same argument that made the log viewer a page, and it is what lets that page poll only the projects worth polling.
   */
  debugBridgeOverview(): Promise<(Record<string, unknown>)[]>;
  /**
   * P3-20 — what `laradock ship` does. The analysis calls it the long-horizon differentiator because the container lineage makes it possible and no native-binary competitor can follow. It is also the item with a real question inside it, answered by looking at what the images actually contain rather than by assuming.
   */
  releasePlan(name: string, tag?: string): Promise<ReleasePlan>;
  releaseBuild(name: string, tag?: string): Promise<ReleaseResult>;
  /**
   * H-1. MUST re-run the verification rather than remember it from the build: the interval between building and pushing is exactly where somebody could retag something else onto the name.
   */
  releasePushPlan(name: string, tag?: string): Promise<PushPlan>;
  /**
   * MUST re-check on the way out — the plan the caller was shown crossed the IPC boundary and came back, and a check that only runs on the way out is not a check.
   */
  releasePush(name: string, tag?: string): Promise<PushPlan>;
  /**
   * A deployment recipe for the built image. Returned as TEXT, not written: where it belongs is the user's decision. MUST carry no source mount, no debugger, no database container, and variable NAMES with empty values — a recipe with credentials in it is a .env wearing a different extension, and this file is meant to be committed.
   */
  releaseRecipe(name: string, tag?: string): Promise<string>;
  releaseSave(name: string, tag?: string, path: string): Promise<number>;
  releaseLoad(path: string): Promise<string[]>;
  /**
   * P3-17 named Blackfire and SPX, and both are the wrong door. Blackfire ships a template already and needs an ACCOUNT — a signup wall in a local development tool. SPX, XHProf and Excimer are not in contracts/php-extensions.json (only xdebug is), so adding one is a change to a contract shared with upstream, the same class of decision as the Mailpit swap. **Xdebug is already a profiler**: xdebug.mode=profile writes cachegrind files, the extension is in the catalog, and the overlay that sets XDEBUG_MODE already belongs to this app. That is the one route with no contract change attached.
   */
  profilerStatus(name: string): Promise<ProfilerStatus>;
  profilerSetMode(name: string, mode: 'debug' | 'profile'): Promise<ProfilerStatus>;
  profilerRead(name: string, id: string): Promise<ProfileReport>;
  profilerDelete(name: string, id: string): Promise<void>;
  /**
   * Profiling fills a disk fast — the 200,000-iteration loop that shaped the parser produced a 10 MB file from one run — so "clear these" has to be one button, not sixty.
   */
  profilerClear(name: string): Promise<Record<string, unknown>>;
  /**
   * P3-19 was "a tinker quick action — the PTY exists, so it is nearly free". It is, and on its own it is also one button. What is worth building is the set it belongs to: artisan tinker, artisan migrate, composer install, npm install, wp shell. Each of those today means opening a terminal, remembering the container name and typing `docker exec -it stackvo-<project> …`.
   */
  quickCommands(name: string): Promise<QuickCommand[]>;
  /** Runs one, by id. */
  quickCommandRun(name: string, id: string): Promise<OperationId>;
  /**
   * F-5, and §5.5 held it as a decision rather than a task: quickcmd refused an in-app REPL in writing — "a second, worse REPL next to the one they already have configured" — and reversing a refusal is not something a commit does quietly. The refusal stands for a LINE repl and this is not one. A snippet is twenty lines you edit: write a query, run it, change line three, run it again. In tinker that is retyping. `tinker` still opens the user's own terminal, and the two are split by what the person is doing rather than ranked.
   */
  replRunners(name: string): Promise<ReplRunner[]>;
  /**
   * Run the snippet against the booted application and hand back everything about the run — both streams, the exit code, the duration.
   */
  replRun(name: string, runner: string, code: string): Promise<ReplRun>;
  /**
   * The snippet you ran ten minutes ago, back in the editor. Without it the workbench is a text box that forgets, which is the terminal REPL's weakness rather than an improvement on it.
   */
  replHistory(name: string): Promise<ReplSnippet[]>;
  /**
   * Forget them. A snippet can hold a customer's id or a token somebody pasted in, and a history with no way to clear it is one people learn not to use.
   */
  replHistoryClear(name: string): Promise<ReplSnippet[]>;
  /**
   * P2-15 was written as "proxy the Node dev server through Traefik with HMR", which sounds like a routing change. Reading node.sh against the other five server generators turns up something larger: **a node project has no bind mount at all**. nginx/caddy/apache/swoole/frankenphp all call generate_common_volumes; node.sh calls nothing, and its Dockerfile does `COPY . .` and `RUN npm install` at build time. The container holds a SNAPSHOT of the source. Hot reload was not misconfigured — it was structurally impossible, and no amount of WebSocket plumbing helps when there is nothing to reload.
   */
  devserverStatus(name: string): Promise<DevServerStatus>;
  devserverSet(name: string, enabled: boolean, command?: string): Promise<DevServerStatus>;
  /**
   * The other half of P2-12. The folder half shipped with adoption in Sprint 4 — detection infers runtime, server and document root from artisan/wp-config.php/composer.json. What it cannot see is everything the person who wrote the compose file already decided: the PHP version, the domain, the extensions, and — with no equivalent in any marker file — WHICH BACKING SERVICES the project needs. A compose file with mysql:8.0 and redis:7.2 is a statement about the stack; adoption alone leaves the developer to rediscover it from a stack trace about a refused connection.
   */
  migrateScan(name: string): Promise<MigrationPlan>;
  migrateApply(name: string, spec?: ProjectSpec, services?: boolean): Promise<MigrationPlan>;
  /**
   * The roadmap said "turn the commit-friendly stackvo.json into a flow". Read against the code that framing is wrong: stackvo.json needs no flow — it is already in the project directory, already schema-validated, and a teammate who clones the repo already has it. What they do NOT get is the STACK: which of the twenty services are enabled and at which versions. That lives in .env, the one file nobody commits, because it is also where every password is. So the clone succeeds, the manifest is perfect, and the project still will not start until somebody says "you need MySQL 8.0, Redis and Elasticsearch on". That sentence is the preset.
   */
  presetExport(name?: string): Promise<Preset>;
  presetSave(path: string, name?: string): Promise<string>;
  /**
   * Importing a colleague's stack must never be a blind write over your own. Same shape as hosts_plan/hosts_apply and cert_plan/cert_apply — the pattern this app already uses everywhere a change leaves its own process.
   */
  presetPlan(path: string): Promise<PresetPlan>;
  presetApply(path: string): Promise<PresetPlan>;
  /**
   * HTTPS was already working and entirely invisible. `core/cli/utils/generate-ssl-certs.sh` issues an mkcert wildcard covering stackvo.loc and every project domain, and traefik.sh installs it as the default certificate — but nothing in the app could say whether mkcert was installed, whether the CA was trusted, whether the certificate had expired, or which domains it actually covered. A user whose new project shows a browser warning had no way to learn why.
   */
  certStatus(): Promise<CertStatus>;
  /**
   * Same order as hosts_plan: say what would change before doing it. Reissuing replaces the file Traefik is serving, and `installCa` can touch the system trust store, so neither should be the first thing the user learns about it.
   */
  certPlan(): Promise<CertPlan>;
  /**
   * Reissues the wildcard certificate for the domains the projects actually have, and — when the CA is not trusted yet — installs it. Without this the certificate goes stale the moment a project is created, and on Linux and Windows the CA is never trusted at all: the Bash helper's trust step returns early on anything but macOS, so those users get a browser warning and no explanation.
   */
  certApply(install_ca: boolean): Promise<CertPlan>;
  certTrustInTerminal(): Promise<void>;
  workspacePick(): Promise<Workspace>;
  /** ProjectDetail.vue currently re-fetches the whole list and filters client-side. */
  projectGet(name: string): Promise<Project>;
  /**
   * The create half of P0-1 (detection/adoption shipped first). Runs the framework's own installer in a throwaway container — composer create-project, wp-cli, create-next-app — so nothing is installed on the host and only the bind-mounted project directory survives.
   */
  projectScaffold(name: string, template: 'laravel' | 'symfony' | 'cakephp' | 'yii' | 'codeigniter' | 'laminas' | 'slim' | 'wordpress' | 'drupal' | 'prestashop' | 'typo3' | 'tina' | 'nextjs' | 'nuxt' | 'vue' | 'react' | 'svelte' | 'astro' | 'nest' | 'angular' | 'django' | 'rails' | 'gin' | 'echo' | 'flask' | 'fastapi' | 'sinatra' | 'rocket'): Promise<OperationId>;
  /**
   * The clone option is hidden without it. A webview cannot answer "is a program installed", and the answer has to survive an app launched from the Dock, which inherits launchd's PATH rather than a login shell's — the same class of fact that made $LANG useless for locale detection.
   */
  gitAvailable(): Promise<boolean>;
  /**
   * The only way a repository reached the workspace was the user cloning it in a terminal and then adopting the folder. The clone is the one step the app was not doing, and it is the first one.
   */
  projectClone(url: string, name?: string): Promise<Record<string, unknown>>;
  /**
   * project_adopt refuses a directory that already has a stackvo.json, and refuses correctly — it must not overwrite settings somebody chose. But that was only half an answer: the compose files, the hosts entry and the certificate had not happened either, and nothing else did them. The manifest watcher reports a change and regenerates nothing, on purpose.
   */
  projectRegister(name: string): Promise<OperationId>;
  /**
   * N. Everything a project can say about worktrees before a dialog opens: whether git is here, whether the directory is a repository, which branches exist and which of them are already checked out somewhere, which database instances a branch could be given a database on, and the worktrees this project already has. One call rather than five, because the answer decides whether the button is even drawn.
   */
  worktreeSupport(name: string): Promise<WorktreeSupport>;
  /**
   * Every worktree in the workspace, so the projects list can say `branch of shop` on a row instead of showing two unrelated siblings. Asking per project would be one command per row.
   */
  worktreeList(): Promise<WorktreeRow[]>;
  /**
   * A worktree creates a directory, a hostname, a container and a database at once, and the only moment any of those can be argued with is before they exist. The same plan-then-apply pair as hosts_plan/hosts_apply and db_move_plan/db_move_apply.
   */
  worktreePlan(name: string, branch: string, options?: Record<string, unknown>): Promise<WorktreePlan>;
  /**
   * N — the item nothing else in this space does. `git worktree add` gives a branch its own directory; this gives that directory its own subdomain, its own database and its own environment, so two branches of one application run side by side.
   */
  worktreeCreate(name: string, branch: string, options?: Record<string, unknown>): Promise<OperationId>;
  /**
   * The way back. A worktree left behind is a directory that looks like a project, a branch git will not check out anywhere else, and a database nothing points at.
   */
  worktreeRemove(name: string, options?: Record<string, unknown>): Promise<OperationId>;
  /**
   * A worktree's own environment variables. site_save cannot serve this: it writes .stackvo/site.json, which is inside the checkout, and on a worktree the checkout is a branch somebody else is working on.
   */
  worktreeEnvSet(name: string, env: Record<string, unknown>): Promise<Record<string, unknown>>;
  /**
   * Webhook testing (Stripe, GitHub) had no answer: myapp.loc does not exist on the internet. This reports every running tunnel sidecar and its assigned public URL.
   */
  tunnelStatus(): Promise<TunnelStatus[]>;
  /**
   * Starts a cloudflared quick-tunnel sidecar on the stack's network, forwarding a random trycloudflare.com URL to the project's container. No account, no token — and no host-binary competitor can attach to the container network at all.
   */
  tunnelStart(name: string): Promise<OperationId>;
  tunnelStop(name: string): Promise<void>;
  /**
   * M-3. Two features hand out an address whose whole point is that it is opened on ANOTHER device — the LAN name from lan_status and the public URL from tunnel_status — and both are long, both contain either a dashed IP address or four random Cloudflare words, and the only way to get one onto a phone was to type it. That is the moment somebody gives up and uses the desktop browser's device emulation instead, which is not the same thing and is exactly the class of bug it fails to show.
   */
  qrEncode(text: string): Promise<Record<string, unknown>>;
  /**
   * M-12, which sat on the list as 'an OAuth callback for .loc' and was not defined enough to build. Reading what the providers require settles it: a redirect URI is a BROWSER REDIRECT, not a fetch — the provider sends a 302 and never resolves the hostname — so https://shop.loc/auth/callback works for the flow, because the browser is on this machine, the name is in this machine's hosts file and the certificate is issued by a CA it trusts. What varies is whether the provider will accept the string at REGISTRATION time, and that is a per-provider rule that is invisible at their console.
   */
  oauthCallbacks(name: string, path: string): Promise<Record<string, unknown>>;
  /**
   * M-11. Testing a payment flow means Stripe reaching the application, and shop.loc does not exist on the internet.
   */
  stripeStatus(): Promise<Record<string, unknown>[]>;
  /**
   * A credential needs somewhere to go that is not a file in the workspace, and this app already decided where that is.
   */
  stripeKeySet(name: string, key: string | null): Promise<boolean>;
  /** An operation because the first start pulls the Stripe image. */
  stripeStart(name: string, path: string, events: string[]): Promise<OperationId>;
  /** Turning it off has to be as easy as turning it on. */
  stripeStop(name: string): Promise<void>;
  /**
   * M-4. Every rival in the category ships one and it is the address people bookmark. StackVo has had the NAME for it since the beginning with nothing answering on it: core_domains already writes the bare suffix into the hosts file and certs::required_domains already issues for it, so opening https://<suffix> got Traefik's own 404 — a name the app went out of its way to make resolve, serving nothing.
   */
  landingStatus(): Promise<Record<string, unknown>>;
  /**
   * An operation rather than a mutation: the first start pulls nginx, which belongs in the operation console rather than behind a frozen button.
   */
  landingStart(): Promise<OperationId>;
  /** Turning it off has to be as easy as turning it on. */
  landingStop(): Promise<void>;
  /**
   * The sidecar serves a FILE, so starting a project after the page was written leaves it stale with nothing having stopped. Rewriting is therefore a different action from serving, and one button doing both would restart a container to update a list.
   */
  landingRefresh(): Promise<unknown>;
  /**
   * Which workers this project can offer, from its files alone: artisan offers 'queue' and 'scheduler', laravel/horizon in composer.json adds 'horizon'. A Node project gets an empty list, not an error.
   */
  workerOptions(name: string): Promise<string[]>;
  /**
   * A Laravel app runs queue:work, the scheduler and Horizon beside the web server; locally they live in a forgotten terminal tab, which is why "my job never ran" is a support staple. This reports every worker sidecar with its restart count — the self-heal made visible; a large number is a crash loop, not a success story.
   */
  workerStatus(): Promise<WorkerStatus[]>;
  workerStart(name: string, kind: 'queue' | 'scheduler' | 'horizon'): Promise<void>;
  workerStop(name: string, kind: 'queue' | 'scheduler' | 'horizon'): Promise<void>;
  /**
   * Pre-flight the new-project form against project.schema.json + php-extensions.json before anything touches disk. Today a bad extension name is only discovered when the Docker build fails minutes later.
   */
  projectValidate(spec: ProjectSpec): Promise<ValidationReport>;
  projectCreate(spec: ProjectSpec): Promise<OperationId>;
  projectDelete(name: string, remove_files: boolean): Promise<void>;
  /**
   * Competitive review §L. XAMPP has been frozen on PHP 8.2 since 2023 and lost its add-on ecosystem in September 2025; Laragon went commercial in 2025 and was forked. Those are the two largest installed bases in the category and every rival is courting them — EnvKit imports Laragon in bulk, ForgeKit lists six sources. StackVo could read neither.
   */
  importsScan(): Promise<Record<string, unknown>[]>;
  /**
   * The same, for an installation somewhere this app did not think to look — the well-known paths are installer defaults and people move things.
   */
  importsScanAt(source: 'xampp' | 'laragon' | 'mamp' | 'valet' | 'sail', path: string): Promise<Install | null>;
  /**
   * Brings one site into the workspace. The generator bind-mounts ${PROJECTS}/<name>, so a project lives under the projects directory or it does not exist — an import is a file operation, and this is that half only.
   */
  importsTake(path: string, name: string, move: boolean): Promise<string>;
  /**
   * `project_create` refuses when the directory already exists, so a folder cloned into projects/ could not be brought under management at all — the only way in was writing stackvo.json by hand. On the checkout this was written against, 11 of 21 directories under projects/ were in that state: real code, unmanaged.
   */
  projectAdoptable(): Promise<Adoptable[]>;
  /**
   * Writes stackvo.json for a directory that is already there, then regenerates. The counterpart of project_create, which requires the directory to be absent.
   */
  projectAdopt(name: string, spec: ProjectSpec, overrides: Record<string, unknown>): Promise<OperationId>;
  projectManifestRead(name: string): Promise<ProjectManifest>;
  /**
   * Editing an existing project is impossible today — the UI can only create and delete. MUST honour the write rules in project.schema.json (extensions last, one runtime block).
   */
  projectManifestWrite(name: string, manifest: ProjectManifest): Promise<void>;
  /**
   * B-2. A committed manifest is what makes a checkout reproducible and is exactly why there was nowhere to say “on this machine, PHP 8.3”. Reads the overlay beside it.
   */
  projectLocalRead(name: string): Promise<LocalOverride>;
  /**
   * Text rather than a parsed object: this is a file somebody types, and a struct round-trip would reformat what they wrote. Empty text removes the file — “no overrides” and “an empty overrides file” are the same state and only one of them is something to wonder about in a directory listing. MUST reject `name` and `runtime`, and MUST validate the merged document rather than the fragment.
   */
  projectLocalWrite(name: string, text: string): Promise<LocalOverride>;
  /**
   * B-3. A hook is a command from a repository somebody cloned, so what would run has to be readable before it does.
   */
  projectHooksPlan(name: string): Promise<HookPlan[]>;
  /**
   * Approves this project's HOST commands exactly as they are now. The digest is sent back rather than recomputed server-side, and that round trip is the point: it is a receipt for the list the person actually read. MUST refuse when the manifest changed between the screen being drawn and the button being pressed — that refusal is what makes this consent rather than a checkbox. Container steps are never gated: the container already runs the repository's code.
   */
  projectHooksApprove(name: string, digest: string): Promise<HookPlan[]>;
  /** Withdraws approval. Takes no digest — you may always revoke. */
  projectHooksRevoke(name: string): Promise<HookPlan[]>;
  /**
   * E-3. `shop.loc` exists in exactly one place — this machine's /etc/hosts — so opening the site on a real phone has never been possible without editing a file on that phone. This writes the intent; `lan_status` derives the name.
   */
  projectLanShare(name: string, enabled: boolean): Promise<ProjectManifest>;
  /**
   * The screen has to be able to say three different things: here is the address a phone can use, this machine has no address to offer, and the name baked into the compose file is from a network this laptop is no longer on.
   */
  lanStatus(): Promise<unknown>;
  /**
   * Competitive review B-1: herd.yml, .lerd.yaml and .ddev/config.yaml all describe the whole environment and are committed, so a teammate clones and runs one command. stackvo.json carried the project and a preset carried the stack, and the two never met. `services` in the manifest is the missing half; this reports what the repository declares, what this machine gives it, and the diff.
   */
  projectRequirements(name: string): Promise<Record<string, unknown>>;
  /**
   * Turns the declaration into an enabled stack, through the plan-then-apply path a preset import already uses rather than a second one with its own rules.
   */
  projectRequirementsApply(name: string): Promise<unknown>;
  /**
   * Writes the `services` list into stackvo.json, which is the file that gets committed — the point of the feature is that the next person to clone does not have to be told.
   */
  projectRequirementsDeclare(name: string, services: string[]): Promise<unknown>;
  updaterStatus(): Promise<Record<string, unknown>>;
  /**
   * MIT, BSD, ISC and Apache-2.0 all require the copyright notice and the licence text to travel with the software. A NOTICE.md in a source repository does not reach the person who received a .dmg, and pointing them at the repository is the same answer as not shipping it.
   */
  licencesNotice(): Promise<string>;
  /**
   * A fleet of machines has settings somebody other than the person at the keyboard cares about — the domain suffix, the web server, and on a network without Docker Hub the registry every image is pulled from. Without this the Settings panes can only grey a field out, and a greyed-out field with no reason reads as a broken app rather than a managed one.
   */
  policyStatus(): Promise<Record<string, unknown>>;
  /**
   * The Settings pane has to draw a row per credential saying where it lives, and it has to know before it offers a Move button whether this machine has a keystore at all — a headless Linux box with no Secret Service is a real machine somebody runs this on.
   */
  secretsStatus(): Promise<Record<string, unknown>>;
  /**
   * On a company machine ~/.stackvo/.env is backed up, synced and DLP-scanned, and it holds database passwords in plain text (readiness review §5.2). This moves one into Keychain / Credential Manager / Secret Service and leaves `keychain:<entry>` in its place.
   */
  secretMove(key: string): Promise<void>;
  /**
   * The way back. A user may only discover the CLI incompatibility after moving a key, and without this the only way out is hand-editing .env with a value the app will not show them.
   */
  secretRestore(key: string): Promise<void>;
  /**
   * The README asked the reader to find their assistant's configuration file, work out its shape and paste a JSON block into it with a path they had to supply themselves. Competitive review K-1 — every rival with an MCP server installs itself. This is the read half: which clients are on this machine, which already point at the server, and whether the binary exists to point at.
   */
  agentsStatus(): Promise<Record<string, unknown>>;
  /**
   * Writes the `stackvo` MCP entry into one client's configuration file, so registering the server is a click rather than hand-edited JSON. Returns the path written.
   */
  agentsInstall(client: string, allowWrites: boolean): Promise<string>;
  /** The way back out. A registration that can only be added is one people avoid adding. */
  agentsRemove(client: string): Promise<string>;
  /**
   * The desktop's own accent colour, so the app can match it instead of shipping a brand colour that clashes with everyone's system theme.
   */
  systemAccent(): Promise<Record<string, unknown>>;
  logsInfo(): Promise<Record<string, unknown>>;
  /**
   * Settings could open the log folder and nothing more, which leaves the reporter to find the right file among seven daily ones, know that the doctor output is separate, and remember the version and platform. One archive gets the maintainer the same set every time.
   */
  diagnosticsBundle(path: string): Promise<Record<string, unknown>>;
  /**
   * The window and the tray have to open in the same language, and neither could read the machine's on its own. The tray fell back to $LANG, which a Finder-launched app does not have; the window fell back to navigator.language, which in a WKWebView answers from the bundle's localised resources — this app ships none. Both defaulted to English on a Turkish machine.
   */
  localeGet(): Promise<'en' | 'tr'>;
  /**
   * The language set used to be a constant in three places — locale.rs, i18n/index.js and the tray's fallback table — so a third language meant a source change and a rebuild. Nobody who can actually translate this app can do any of that, which is why M-7 sat on the list as '~2,000 strings': the strings were never the blocker, the rebuild was.
   */
  localePacks(): Promise<Record<string, unknown>[]>;
  /** The messages, for the front end to merge over English. */
  localePackRead(tag: string): Promise<Record<string, unknown>>;
  /**
   * How 'start a translation' works: the front end sends the English catalogue as the starting point.
   */
  localePackWrite(tag: string, messages: Record<string, unknown>): Promise<string>;
  /** A language somebody installed has to be removable. */
  localePackDelete(tag: string): Promise<void>;
  trayRelabel(labels?: Record<string, unknown>): Promise<void>;
  appsAvailable(): Promise<Record<string, unknown>>;
  windowCloseAction(action: 'tray' | 'quit' | 'stopAndQuit', remember: boolean): Promise<void>;
  containerStatsHistory(name: string): Promise<StatSample[]>;
  containersStartAll(): Promise<OperationId>;
  containersStopAll(): Promise<OperationId>;
  containersRestartAll(): Promise<OperationId>;
  composeUpProject(name: string): Promise<OperationId>;
  /** Same reason as compose_down. */
  composeRestart(): Promise<OperationId>;
  openInEditor(path: string): Promise<void>;
  /**
   * A project's own domain deserves the browser the developer works in, not whatever the OS last associated with https. The opener plugin has no notion of which browser.
   */
  openInBrowser(url: string): Promise<void>;
  openFolder(path: string): Promise<void>;
  prefsGet(): Promise<Preferences>;
  prefsSet(patch: Preferences): Promise<void>;
  /**
   * The migration gate. A fixture suite only covers the cases someone thought to write down; this renders every generated file with the Rust port against the user's real projects and real .env, so a divergence surfaces on their machine before anything depends on it.
   */
  generatorVerify(): Promise<GeneratorReport>;
  /**
   * Runs the Rust generator port alongside the Bash one so its output can be compared before it replaces anything. `matchesBashOutput` is the differential check, live against the user's own projects rather than only against fixtures.
   */
  projectDockerfilePreview(name: string, strict: boolean): Promise<DockerfilePreview>;
  ptyOpen(target: PtyTarget, cols: number, rows: number): Promise<unknown>;
  ptyWrite(session_id: string, data: string): Promise<void>;
  ptyResize(session_id: string, cols: number, rows: number): Promise<void>;
  ptyClose(session_id: string): Promise<void>;
  terminalOpenExternal(target: PtyTarget): Promise<void>;
}

export declare const api: StackvoApi;

export declare class StackvoError extends Error {
  code: string;
  hint?: string;
  hintKey?: string;
  details?: Record<string, unknown>;
}

export declare function call<T = unknown>(
  command: string,
  args?: Record<string, unknown>
): Promise<T>;

export declare function asList<T>(value: T[] | null | undefined): T[];
