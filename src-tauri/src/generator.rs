//! Porting the generator to Rust.
//!
//! This is the piece that decides whether StackVo Desktop can ever run on
//! Windows natively: `core/cli/` is 4.5k lines of Bash, and while a bundled
//! sidecar works on macOS and Linux, it is exactly what forces the WSL2
//! requirement the README documents.
//!
//! The port is verified DIFFERENTIALLY rather than by inspection: `resolve`
//! and `render_dockerfile` are checked byte-for-byte against the Dockerfiles
//! the Bash generator has already written to `generated/projects/` in a real
//! checkout. A generator that produces "basically the same" output is a
//! generator that silently changes people's images.
//!
//! Scope: PHP + nginx and Node today — the two runtimes that actually have
//! generators (CONFLICTS.md C-02). Apache/Caddy/FrankenPHP/Swoole reuse the
//! same `resolve` and differ only in their Dockerfile template.

use crate::contracts::{cmp_php_version, php_extensions};
use crate::manifest::Manifest;
use std::cmp::Ordering;

/// Extensions PHP 8.0+ ships enabled. Skipped entirely — no install line and
/// no apt packages, matching `project.sh`, which `continue`s before it reaches
/// `get_extension_packages`.
const BUILTIN: [&str; 25] = [
    "core",
    "date",
    "pcre",
    "reflection",
    "spl",
    "standard",
    "random",
    "zlib",
    "tokenizer",
    "json",
    "filter",
    "hash",
    "session",
    "ctype",
    "iconv",
    "fileinfo",
    "phar",
    "posix",
    "openssl",
    "dom",
    "xml",
    "simplexml",
    "xmlreader",
    "xmlwriter",
    "libxml",
];

/// docker-php-ext-install batching. The order is load-bearing: `pdo` must be
/// built before the `pdo_*` drivers, and xmlreader/xmlwriter need dom's headers
/// present in the same layer.
fn batch_of(ext: &str) -> usize {
    match ext {
        "pdo" => 0,
        "pdo_mysql" | "pdo_pgsql" | "pdo_sqlite" | "mysqli" | "pgsql" => 1,
        "dom" | "xml" | "simplexml" | "xmlreader" | "xmlwriter" | "xmlrpc" => 2,
        _ => 3,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Sorted and de-duplicated.
    pub apt_packages: Vec<String>,
    pub configure: Vec<String>,
    /// Manifest order, builtins and PECL removed.
    pub docker_ext: Vec<String>,
    /// Manifest order, PECL dependencies prepended.
    pub pecl: Vec<(String, Option<String>)>,
    /// As requested, WITHOUT dependency expansion. FrankenPHP installs through
    /// its own `install-php-extensions`, which resolves dependencies itself —
    /// so it gets this list, not the expanded one.
    pub pecl_requested: Vec<String>,
    /// Extensions the Bash generator drops silently, with the reason. In strict
    /// mode these are errors; in compat mode they are what makes the output
    /// match byte-for-byte.
    pub skipped: Vec<(String, String)>,
}

/// Resolve a manifest's extension list into an install plan.
///
/// `strict` is the v1 behaviour change: the Bash generator answers "unknown
/// extension" and "removed in this PHP version" with `continue`, so a typo
/// produces an image that builds fine and lacks the extension. Strict mode
/// turns those into errors; compat mode reproduces the silence, and is what the
/// differential test runs.
pub fn resolve(php_version: &str, extensions: &[String], strict: bool) -> Result<Plan, String> {
    let matrix = &php_extensions().extensions;

    let mut apt = Vec::new();
    let mut configure = Vec::new();
    let mut docker_ext = Vec::new();
    let mut pecl_names: Vec<String> = Vec::new();
    let mut skipped = Vec::new();

    for ext in extensions {
        let skip = |reason: &str, skipped: &mut Vec<(String, String)>| {
            skipped.push((ext.clone(), reason.to_string()));
        };

        let Some(spec) = matrix.get(ext) else {
            if strict {
                return Err(format!("`{ext}` is not in the extension matrix"));
            }
            // The Bash `*)` catch-all returns an empty package list, so an
            // unknown name reaches docker-php-ext-install verbatim and fails
            // the build there. Reproduce that rather than inventing a skip.
            docker_ext.push(ext.clone());
            continue;
        };

        if spec.install == "composer" {
            skip("Composer package, not an extension", &mut skipped);
            continue;
        }

        if BUILTIN.contains(&ext.as_str()) {
            skip("built-in since PHP 8.0", &mut skipped);
            continue;
        }

        if let Some(removed) = &spec.removed_in {
            if cmp_php_version(php_version, removed) != Ordering::Less {
                if strict {
                    return Err(format!("`{ext}` was removed in PHP {removed}"));
                }
                skip(&format!("removed in PHP {removed}"), &mut skipped);
                continue;
            }
        }

        if spec.install == "special" {
            if strict {
                return Err(format!(
                    "`{ext}` needs an install path v1 does not implement"
                ));
            }
            skip("requires special setup", &mut skipped);
            continue;
        }

        if let Some(min) = &spec.min_php {
            if cmp_php_version(php_version, min) == Ordering::Less {
                if strict {
                    return Err(format!("`{ext}` needs PHP >= {min}"));
                }
                skip(&format!("needs PHP >= {min}"), &mut skipped);
                continue;
            }
        }

        apt.extend(spec.packages.iter().cloned());
        if let Some(cmd) = &spec.configure {
            if !cmd.is_empty() {
                configure.push(cmd.clone());
            }
        }

        if spec.install == "pecl" {
            pecl_names.push(ext.clone());
        } else {
            docker_ext.push(ext.clone());
        }
    }

    // `sort -u`.
    apt.sort();
    apt.dedup();

    // PECL dependencies come first, then the requested extensions, each once.
    let mut ordered: Vec<String> = Vec::new();
    for ext in &pecl_names {
        if let Some(spec) = matrix.get(ext) {
            for dep in &spec.pecl_dependencies {
                if !ordered.contains(dep) {
                    ordered.push(dep.clone());
                }
            }
        }
    }
    for ext in &pecl_names {
        if !ordered.contains(ext) {
            ordered.push(ext.clone());
        }
    }

    let pecl: Vec<(String, Option<String>)> = ordered
        .into_iter()
        .map(|ext| {
            let version = matrix.get(&ext).and_then(|spec| {
                spec.pecl_versions
                    .get(php_version)
                    .or_else(|| spec.pecl_versions.get("default"))
                    .filter(|v| !v.is_empty() && v.as_str() != "latest")
                    .cloned()
            });
            (ext, version)
        })
        .collect();

    Ok(Plan {
        apt_packages: apt,
        configure,
        docker_ext,
        pecl,
        pecl_requested: pecl_names,
        skipped,
    })
}

/// The `docker-php-ext-install` RUN lines, batched.
fn extension_install_block(plan: &Plan) -> String {
    if plan.docker_ext.is_empty() {
        return String::new();
    }

    let mut out = String::from("# Install PHP extensions\n");
    for batch in 0..4 {
        let group: Vec<&String> = plan
            .docker_ext
            .iter()
            .filter(|e| batch_of(e) == batch)
            .collect();
        if group.is_empty() {
            continue;
        }
        out.push_str("RUN docker-php-ext-install");
        for ext in group {
            out.push(' ');
            out.push_str(ext);
        }
        out.push('\n');
    }
    out.push('\n');
    out
}

fn pecl_install_block(plan: &Plan) -> String {
    if plan.pecl.is_empty() {
        return String::new();
    }

    let mut out = String::from(
        "# Install PECL extensions\n# Note: Versions are specified for stability, errors are tolerated\n\n",
    );
    for (ext, version) in &plan.pecl {
        let target = match version {
            Some(v) => format!("{ext}-{v}"),
            None => ext.clone(),
        };
        out.push_str("RUN --mount=type=cache,target=/tmp/pear,sharing=locked \\\n");
        out.push_str(&format!("    pecl install {target} \\\n"));
        out.push_str(&format!("    && docker-php-ext-enable {ext}\n\n"));
    }
    out
}

fn apt_block(packages: &[String], cached: bool) -> String {
    if packages.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    if cached {
        out.push_str("# Install System Packages with BuildKit cache\n");
        out.push_str("RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \\\n");
        out.push_str("    --mount=type=cache,target=/var/lib/apt,sharing=locked \\\n");
        out.push_str("    apt-get update && apt-get install -y \\\n");
    } else {
        out.push_str("# Install System Packages\n");
        out.push_str("RUN apt-get update && apt-get install -y \\\n");
    }
    for pkg in packages {
        out.push_str(&format!("    {pkg} \\\n"));
    }
    out.push_str("    && rm -rf /var/lib/apt/lists/*\n");
    out
}

fn tools_block(tools: &[String], composer_version: &str, nodejs_version: &str) -> String {
    let mut out = String::from("\n# Install Development Tools\n");
    for tool in tools {
        match tool.as_str() {
            "composer" => {
                out.push_str("# Install Composer\n");
                out.push_str(&format!(
                    "COPY --from=composer:{composer_version} /usr/bin/composer /usr/bin/composer\n"
                ));
            }
            "nodejs" => {
                out.push_str(&format!("\n# Install Node.js {nodejs_version}.x\n"));
                out.push_str(&format!(
                    "RUN curl -fsSL https://deb.nodesource.com/setup_{nodejs_version}.x | bash - \\\n"
                ));
                out.push_str("    && apt-get install -y nodejs \\\n");
                out.push_str("    && rm -rf /var/lib/apt/lists/*\n");
            }
            "git" | "wget" | "unzip" => {
                out.push_str(&format!(
                    "# Install {}\nRUN apt-get update && apt-get install -y {tool} && rm -rf /var/lib/apt/lists/*\n",
                    tool[..1].to_uppercase() + &tool[1..]
                ));
            }
            // Unknown tool names produce nothing, matching the Bash `*)` case.
            _ => {}
        }
    }
    out
}

/// Everything the nginx Dockerfile template needs from `.env`.
pub struct ToolchainOptions {
    pub tools: Vec<String>,
    pub apt_packages: Vec<String>,
    pub composer_version: String,
    pub nodejs_version: String,
}

/// The five web servers that have generators.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Server {
    Nginx,
    Apache,
    Caddy,
    FrankenPhp,
    Swoole,
}

impl Server {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "nginx" => Some(Server::Nginx),
            "apache" => Some(Server::Apache),
            "caddy" => Some(Server::Caddy),
            "frankenphp" => Some(Server::FrankenPhp),
            "swoole" => Some(Server::Swoole),
            _ => None,
        }
    }

    /// `FROM php:<version>-<variant>`. Swoole is the odd one: it IS the HTTP
    /// server, so it runs on the CLI image rather than fpm or apache.
    fn base_image(self, php_version: &str) -> String {
        match self {
            Server::Nginx | Server::Caddy => format!("FROM php:{php_version}-fpm"),
            Server::Apache => format!("FROM php:{php_version}-apache"),
            Server::Swoole => format!("FROM php:{php_version}-cli"),
            // FrankenPHP ships its own image with the runtime baked in.
            Server::FrankenPhp => format!("FROM dunglas/frankenphp:1-php{php_version}-bookworm"),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Server::Nginx => "Nginx + PHP-FPM",
            Server::Apache => "Apache + mod_php",
            Server::Caddy => "Caddy + PHP-FPM",
            Server::FrankenPhp => "FrankenPHP (Caddy + embedded PHP)",
            Server::Swoole => "Swoole",
        }
    }

    /// nginx and caddy proxy to PHP-FPM over TCP; the others do not use FPM.
    fn uses_fpm(self) -> bool {
        matches!(self, Server::Nginx | Server::Caddy)
    }
}

const NGINX_PREAMBLE: &str = "# Install Nginx and Supervisord with BuildKit cache\nRUN --mount=type=cache,target=/var/cache/apt,sharing=locked \\\n    --mount=type=cache,target=/var/lib/apt,sharing=locked \\\n    apt-get update && apt-get install -y \\\n    nginx \\\n    supervisor \\\n    && rm -rf /var/lib/apt/lists/*\n\n";

const CADDY_PREAMBLE: &str = "# Install Caddy and Supervisord with BuildKit cache\nRUN --mount=type=cache,target=/var/cache/apt,sharing=locked \\\n    --mount=type=cache,target=/var/lib/apt,sharing=locked \\\n    apt-get update && apt-get install -y \\\n    debian-keyring \\\n    debian-archive-keyring \\\n    apt-transport-https \\\n    curl \\\n    supervisor \\\n    && curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg \\\n    && curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list \\\n    && apt-get update \\\n    && apt-get install -y caddy \\\n    && rm -rf /var/lib/apt/lists/*\n\n";

const FPM_TCP: &str = "# Configure PHP-FPM to listen on TCP port 127.0.0.1:9000\nRUN sed -i 's|listen = .*|listen = 127.0.0.1:9000|' /usr/local/etc/php-fpm.d/www.conf\n";

/// The runtime log-directory entrypoint, parameterised by server name.
fn entrypoint_script(log_dir: &str) -> String {
    format!(
        "# Create entrypoint script to ensure log directories exist at runtime\nRUN echo '#!/bin/bash' > /entrypoint.sh && \\\n    echo 'mkdir -p /var/log/{log_dir} /var/log/php-fpm' >> /entrypoint.sh && \\\n    echo 'touch /var/log/{log_dir}/access.log /var/log/{log_dir}/error.log' >> /entrypoint.sh && \\\n    echo 'chmod 666 /var/log/{log_dir}/*.log' >> /entrypoint.sh && \\\n    echo 'exec \"$@\"' >> /entrypoint.sh && \\\n    chmod +x /entrypoint.sh\n"
    )
}

const SUPERVISORD_CMD: &str = "\nWORKDIR /var/www/html\n\n# Use entrypoint to create log directories before starting supervisord\nENTRYPOINT [\"/entrypoint.sh\"]\nCMD [\"/usr/bin/supervisord\", \"-c\", \"/etc/supervisor/conf.d/supervisord.conf\"]\n";

/// FrankenPHP installs everything through its own `install-php-extensions`
/// helper, which handles core and PECL alike — so the careful batching the
/// other servers need does not apply here.
fn frankenphp_extension_block(plan: &Plan) -> String {
    let mut all: Vec<&str> = plan.docker_ext.iter().map(|s| s.as_str()).collect();
    all.extend(plan.pecl_requested.iter().map(|s| s.as_str()));
    if all.is_empty() {
        return String::new();
    }

    let mut out = String::from(
        "# Install PHP extensions via install-php-extensions\n# FrankenPHP provides this tool (supports both standard and PECL extensions)\nRUN install-php-extensions \\\n",
    );
    for (i, ext) in all.iter().enumerate() {
        let last = i + 1 == all.len();
        out.push_str(&format!("    {ext}{}\n", if last { "" } else { " \\" }));
    }
    out.push('\n');
    out
}

fn swoole_postamble(document_root: &str) -> String {
    let mut out = String::from("\n\n# Expose Swoole port\nEXPOSE 8000\n\n");

    out.push_str("# Swoole fallback server for non-Laravel projects\nRUN { \\\n");
    out.push_str("    echo '<?php'; \\\n");
    out.push_str("    echo '$http = new Swoole\\\\HTTP\\\\Server(\"0.0.0.0\", 8000);'; \\\n");
    out.push_str(&format!(
        "    echo '$http->set([\"document_root\" => \"/var/www/html/{document_root}\", \"enable_static_handler\" => true]);'; \\\n"
    ));
    out.push_str("    echo '$http->on(\"request\", function ($request, $response) {'; \\\n");
    out.push_str("    echo '    ob_start();'; \\\n");
    out.push_str(&format!(
        "    echo '    include \"/var/www/html/{document_root}/index.php\";'; \\\n"
    ));
    out.push_str("    echo '    $response->end(ob_get_clean());'; \\\n");
    out.push_str("    echo '});'; \\\n");
    out.push_str("    echo '$http->start();'; \\\n");
    out.push_str("    } > /swoole-server.php\n\n");

    out.push_str("# Entrypoint: Laravel Octane if available, otherwise standalone Swoole HTTP server\nRUN { \\\n");
    out.push_str("    echo '#!/bin/bash'; \\\n");
    out.push_str("    echo 'cd /var/www/html'; \\\n");
    out.push_str("    echo 'if [ -f artisan ]; then'; \\\n");
    out.push_str("    echo '    exec php artisan octane:start --server=swoole --host=0.0.0.0 --port=8000'; \\\n");
    out.push_str("    echo 'else'; \\\n");
    out.push_str("    echo '    exec php /swoole-server.php'; \\\n");
    out.push_str("    echo 'fi'; \\\n");
    out.push_str("    } > /swoole-entrypoint.sh && chmod +x /swoole-entrypoint.sh\n\n");
    out.push_str("WORKDIR /var/www/html\n\nCMD [\"/swoole-entrypoint.sh\"]\n");
    out
}

/// Render a PHP project's Dockerfile for any of the five servers.
///
/// Byte-for-byte equivalent to the Bash generators. The blank lines and their
/// placement are load-bearing for the differential test — they are artefacts of
/// how the Bash version concatenates heredocs, and reproducing them is what
/// proves nothing else drifted.
pub fn render_php_dockerfile(
    server: Server,
    project_name: &str,
    php_version: &str,
    document_root: &str,
    plan: &Plan,
    opts: &ToolchainOptions,
) -> String {
    let mut out = String::from("# syntax=docker/dockerfile:1.4\n\n");

    // Swoole is the only server with a bespoke header; the rest share one.
    if server == Server::Swoole {
        out.push_str(&format!(
            "# Auto-generated Dockerfile for {project_name}\n# Server: Swoole\n# PHP Version: {php_version}\n# Note: Uses php-cli image - Swoole IS the HTTP server\n{}\n\n",
            server.base_image(php_version)
        ));
    } else {
        out.push_str(&format!(
            "# Auto-generated Dockerfile for {project_name}\n# Web Server: {}\n# PHP Version: {php_version}\n{}\n\n",
            server.label(),
            server.base_image(php_version)
        ));
    }

    match server {
        Server::Nginx => out.push_str(NGINX_PREAMBLE),
        Server::Caddy => out.push_str(CADDY_PREAMBLE),
        _ => {}
    }

    out.push_str(&apt_block(&plan.apt_packages, true));

    // Bash accumulates these as a literal `\nRUN …` string and then runs it
    // through `echo -e`, which expands the escape — so each command is
    // preceded by a blank line, and the block is followed by one more.
    if server != Server::FrankenPhp {
        for cmd in &plan.configure {
            out.push_str(&format!("\nRUN {cmd}"));
        }
        if !plan.configure.is_empty() {
            out.push_str("\n\n");
        }
    }

    if server == Server::FrankenPhp {
        out.push_str(&frankenphp_extension_block(plan));
    } else {
        out.push_str(&extension_install_block(plan));
        out.push_str(&pecl_install_block(plan));
    }

    out.push('\n');
    out.push_str(&apt_block(&opts.apt_packages, false));
    out.push('\n');
    out.push_str(&tools_block(
        &opts.tools,
        &opts.composer_version,
        &opts.nodejs_version,
    ));

    match server {
        Server::Nginx => {
            out.push_str("\n\n");
            out.push_str(FPM_TCP);
            out.push_str("\n# Remove 'main' log format reference from Nginx config\nRUN sed -i 's/ main;/;/' /etc/nginx/nginx.conf\n\n# Disable default Nginx site (it conflicts with our config)\nRUN rm -f /etc/nginx/sites-enabled/default\n\n# Copy Nginx configuration\nCOPY nginx.conf /etc/nginx/conf.d/default.conf\n\n# Copy Supervisord configuration\nCOPY supervisord.conf /etc/supervisor/conf.d/supervisord.conf\n\n");
            out.push_str(&entrypoint_script("nginx"));
            out.push_str(SUPERVISORD_CMD);
        }
        Server::Caddy => {
            out.push_str("\n\n");
            out.push_str(FPM_TCP);
            out.push_str("\n# Copy Caddyfile\nCOPY Caddyfile /etc/caddy/Caddyfile\n\n# Copy Supervisord configuration\nCOPY supervisord.conf /etc/supervisor/conf.d/supervisord.conf\n\n");
            out.push_str(&entrypoint_script("caddy"));
            out.push_str(SUPERVISORD_CMD);
        }
        Server::Apache => {
            out.push_str("\n\n# Enable Apache modules\nRUN a2enmod rewrite\n\n");
            out.push_str(&format!(
                "# Configure Apache DocumentRoot to /var/www/html/{document_root}\nENV APACHE_DOCUMENT_ROOT /var/www/html/{document_root}\nRUN sed -ri -e 's!/var/www/html!${{APACHE_DOCUMENT_ROOT}}!g' /etc/apache2/sites-available/000-default.conf\nRUN sed -ri -e 's!/var/www/!${{APACHE_DOCUMENT_ROOT}}!g' /etc/apache2/apache2.conf /etc/apache2/conf-available/*.conf\n\n"
            ));
            out.push_str("WORKDIR /var/www/html\n");
        }
        Server::FrankenPhp => {
            out.push_str(
                "\n\n# Copy FrankenPHP Caddyfile\nCOPY Caddyfile /etc/caddy/Caddyfile\n\n",
            );
            out.push_str("\nWORKDIR /var/www/html\n\nCMD [\"frankenphp\", \"run\", \"--config\", \"/etc/caddy/Caddyfile\"]\n");
        }
        Server::Swoole => out.push_str(&swoole_postamble(document_root)),
    }

    let _ = server.uses_fpm();
    out
}

/// Swoole forces its own runtime requirements into the plan.
///
/// The server cannot work without the `swoole` extension and `pcntl`, so the
/// generator adds them rather than letting a project fail at runtime for a
/// missing dependency the user could not reasonably know about.
pub fn apply_swoole_requirements(plan: &mut Plan, php_version: &str) {
    if !plan.pecl.iter().any(|(e, _)| e == "swoole") {
        let version = php_extensions().extensions.get("swoole").and_then(|s| {
            s.pecl_versions
                .get(php_version)
                .or_else(|| s.pecl_versions.get("default"))
                .filter(|v| !v.is_empty() && v.as_str() != "latest")
                .cloned()
        });
        plan.pecl.push(("swoole".to_string(), version));

        for dep in ["libssl-dev", "libcurl4-openssl-dev"] {
            if !plan.apt_packages.iter().any(|p| p == dep) {
                plan.apt_packages.push(dep.to_string());
            }
        }
        plan.apt_packages.sort();
        plan.apt_packages.dedup();
    }

    if !plan.docker_ext.iter().any(|e| e == "pcntl") {
        plan.docker_ext.push("pcntl".to_string());
    }
}

/// Render a Node project's Dockerfile.
///
/// Note where this lands: the Bash generator writes it into the project SOURCE
/// directory, not `generated/projects/`, because the build context has to be
/// the real source for `COPY . .` to work. That asymmetry with PHP projects is
/// surprising but load-bearing.
pub fn render_node_dockerfile(project_name: &str, node: &crate::manifest::NodeConfig) -> String {
    let mut out = String::new();
    out.push_str("# Auto-generated by Stackvo — standalone Node.js runtime\n");
    out.push_str(&format!("# Project: {project_name}\n"));
    out.push_str(&format!("FROM node:{}-alpine\n\n", node.version));
    out.push_str("WORKDIR /app\n\n");
    out.push_str("# Copy project source (node_modules excluded via .dockerignore)\nCOPY . .\n\n");
    out.push_str(&format!("# Install dependencies\nRUN {}\n", node.install));

    if let Some(build) = node.build.as_deref().filter(|b| !b.is_empty()) {
        out.push_str(&format!(
            "\n# Build for production\nENV NODE_ENV=production\nRUN {build}\n"
        ));
    }

    out.push_str(&format!(
        "\n# Network configuration (Traefik proxies to this port)\nENV HOST=0.0.0.0\nENV PORT={port}\nEXPOSE {port}\n",
        port = node.port
    ));
    out.push_str(&format!(
        "\n# Start the application\nCMD [\"sh\", \"-c\", \"{}\"]\n",
        node.start
    ));
    out
}

/// Keeps host `node_modules` and build output out of the image — copying them
/// in is how you get an arm64 binary in an amd64 container.
pub const NODE_DOCKERIGNORE: &str = "node_modules\n.output\n.nuxt\ndist\n.git\n.gitignore\n*.log\nnpm-debug.log*\nDockerfile\n.dockerignore\n";

/// Convenience: resolve and render from a manifest in one call.
pub fn render_from_manifest(
    manifest: &Manifest,
    opts: &ToolchainOptions,
    strict: bool,
) -> Result<String, String> {
    if manifest.runtime == "node" {
        let node = manifest
            .node
            .as_ref()
            .ok_or("runtime is node but there is no node block")?;
        return Ok(render_node_dockerfile(&manifest.name, node));
    }

    let php = manifest.php.as_ref().ok_or("not a PHP project")?;
    let server = Server::parse(manifest.server.as_deref().unwrap_or("nginx"))
        .ok_or_else(|| format!("unknown server: {:?}", manifest.server))?;

    let mut plan = resolve(&php.version, &php.extensions, strict)?;
    if server == Server::Swoole {
        apply_swoole_requirements(&mut plan, &php.version);
    }

    Ok(render_php_dockerfile(
        server,
        &manifest.name,
        &php.version,
        manifest.document_root.as_deref().unwrap_or("public"),
        &plan,
        opts,
    ))
}

// ================================================================ compose

/// A project's entry in `generated/docker-compose.projects.yml`.
///
/// Two asymmetries are load-bearing and reproduced exactly:
///
///   - **Build context.** PHP projects use `./projects/<name>`, Node uses
///     `../projects/<name>`. Compose resolves both against the compose file's
///     own directory (`generated/`), so each points at wherever that runtime's
///     Dockerfile actually lands — see CONFLICTS.md C-19.
///   - **Trailing whitespace.** The nginx, apache and caddy heredocs leave
///     trailing spaces on their blank lines; frankenphp, swoole and node do
///     not. Meaningless to YAML, but reproducing it is what proves nothing
///     else drifted.
pub struct ComposeProject<'a> {
    pub name: &'a str,
    pub domain: &'a str,
    pub runtime_server: Option<Server>,
    /// Only Node projects have one; PHP projects proxy to port 80 (or 8000 for
    /// Swoole, which is its own HTTP server).
    pub node_port: Option<u16>,
    pub php_version: Option<&'a str>,
}

/// Traefik uses dots as separators in router names, so a project called
/// `parser.ajans` becomes `parser-ajans`.
pub fn traefik_name(project: &str) -> String {
    project.replace('.', "-")
}

fn traefik_labels(project: &str, domain: &str, port: u16) -> String {
    let name = traefik_name(project);
    format!(
        "    labels:\n\
         \x20     - \"traefik.enable=true\"\n\
         \x20     - \"traefik.http.routers.{name}.rule=Host(`{domain}`)\"\n\
         \x20     - \"traefik.http.routers.{name}.entrypoints=websecure\"\n\
         \x20     - \"traefik.http.routers.{name}.tls=true\"\n\
         \x20     - \"traefik.http.services.{name}.loadbalancer.server.port={port}\"\n"
    )
}

/// Render one service block.
///
/// `host_root` is the absolute path of the StackVo checkout on the host — it
/// goes into the bind mounts verbatim, which is the one place the generated
/// output is machine-specific.
pub fn render_compose_service(project: &ComposeProject, host_root: &str) -> String {
    let name = project.name;

    // Blank lines carry trailing spaces for these three; see the doc comment.
    let (pad, context) = match project.runtime_server {
        None => ("", "../projects"),
        Some(Server::Nginx) | Some(Server::Apache) | Some(Server::Caddy) => ("    ", "./projects"),
        _ => ("", "./projects"),
    };

    // Apache tags its image with the PHP version; everything else uses latest.
    let tag = match (project.runtime_server, project.php_version) {
        (Some(Server::Apache), Some(v)) => v.to_string(),
        _ => "latest".to_string(),
    };

    let mut out = String::new();
    out.push_str(&format!(
        "  {name}:\n    profiles: [\"projects\", \"project-{name}\"]  # --projects for all, --profile project-{{name}} for this project only\n    build:\n      context: {context}/{name}\n      dockerfile: Dockerfile\n    image: stackvo-{name}:{tag}\n    container_name: \"stackvo-{name}\"\n    restart: unless-stopped\n{pad}\n"
    ));

    match project.runtime_server {
        // Node has no source mount: the image is built from the source, and a
        // bind mount over /app would shadow the built output.
        None => {
            let port = project.node_port.unwrap_or(3000);
            out.push_str(&format!(
                "    environment:\n      HOST: 0.0.0.0\n      PORT: {port}\n\n"
            ));
            out.push_str("    networks:\n      - stackvo-net\n\n");
            out.push_str(&traefik_labels(name, project.domain, port));
        }
        Some(server) => {
            // Apache logs to /var/log/apache2; the rest take the whole /var/log.
            let log_target = if server == Server::Apache {
                "/var/log/apache2"
            } else {
                "/var/log"
            };

            out.push_str(&format!(
                "    volumes:\n      - {host_root}/projects/{name}:/var/www/html\n      - {host_root}/logs/projects/{name}:{log_target}\n{pad}\n"
            ));
            out.push_str(&format!("    networks:\n      - stackvo-net\n{pad}\n"));

            // Swoole is its own HTTP server on 8000; the rest sit behind a web
            // server on 80.
            let port = if server == Server::Swoole { 8000 } else { 80 };
            out.push_str(&traefik_labels(name, project.domain, port));
        }
    }

    out
}

/// Render the whole `docker-compose.projects.yml`.
///
/// Services are emitted in name order, which is what the Bash generator's
/// directory iteration produces.
pub fn render_compose_projects(projects: &[ComposeProject], host_root: &str) -> String {
    // Bind mounts carry host paths into Docker, which on Windows means
    // `C:\Users\me` has to become `/c/Users/me`. Identity everywhere else.
    let host_root = crate::paths::to_docker_mount(host_root);
    let host_root = host_root.as_str();

    let mut out = String::from("name: stackvo\n\nservices:\n");

    let mut sorted: Vec<&ComposeProject> = projects.iter().collect();
    sorted.sort_by_key(|p| p.name);

    for project in sorted {
        out.push('\n');
        out.push_str(&render_compose_service(project, host_root));
    }

    out.push_str("\n\nnetworks:\n  stackvo-net:\n    external: true\n");
    out
}

/// Build the compose input list from manifests.
pub fn compose_projects_from<'a>(manifests: &'a [(String, Manifest)]) -> Vec<ComposeProject<'a>> {
    manifests
        .iter()
        .filter_map(|(name, m)| {
            let domain = m.domain.as_deref()?;
            Some(if m.runtime == "node" {
                ComposeProject {
                    name,
                    domain,
                    runtime_server: None,
                    node_port: m.node.as_ref().map(|n| n.port),
                    php_version: None,
                }
            } else {
                ComposeProject {
                    name,
                    domain,
                    runtime_server: Server::parse(m.server.as_deref().unwrap_or("nginx")),
                    node_port: None,
                    php_version: m.php.as_ref().map(|p| p.version.as_str()),
                }
            })
        })
        .collect()
}

// ================================================================ traefik

/// The four services that get a file-provider route.
///
/// Everything else reaches Traefik through container labels in its own compose
/// template. These four do not, because their containers are declared without
/// labels — so if a service ever stops appearing at its domain, this list is
/// the first place to look.
const TRAEFIK_ROUTED: [(&str, u16); 4] = [
    ("rabbitmq", 15672),
    ("mailhog", 8025),
    ("kibana", 5601),
    ("grafana", 3000),
];

/// Inputs for the Traefik files, read from `.env`.
pub struct TraefikOptions<'a> {
    pub tld_suffix: &'a str,
    pub network: &'a str,
    pub ssl_enabled: bool,
    pub redirect_to_https: bool,
    /// service id -> (enabled, subdomain override)
    pub services: Vec<(&'a str, bool, Option<&'a str>)>,
}

impl TraefikOptions<'_> {
    fn subdomain(&self, service: &str) -> String {
        self.services
            .iter()
            .find(|(id, _, _)| *id == service)
            .and_then(|(_, _, url)| *url)
            .unwrap_or(service)
            .to_string()
    }

    fn enabled(&self, service: &str) -> bool {
        self.services
            .iter()
            .any(|(id, on, _)| *id == service && *on)
    }
}

/// `generated/traefik/traefik.yml` — the static configuration.
pub fn render_traefik_config(opts: &TraefikOptions) -> String {
    let mut out = format!(
        "api:\n  dashboard: true\n  insecure: false\n\nproviders:\n  docker:\n    endpoint: \"unix:///var/run/docker.sock\"\n    exposedByDefault: false\n    network: {}\n  file:\n    directory: \"/etc/traefik/dynamic\"\n    watch: true\n\nentryPoints:\n  web:\n    address: \":80\"\n",
        opts.network
    );

    if opts.ssl_enabled {
        if opts.redirect_to_https {
            out.push_str(
                "    http:\n      redirections:\n        entryPoint:\n          to: websecure\n          scheme: https\n",
            );
        }
        out.push_str("  websecure:\n    address: \":443\"\n");
    }

    out
}

/// `generated/traefik/dynamic/routes.yml` — routers, services and TLS.
pub fn render_traefik_routes(opts: &TraefikOptions) -> String {
    let tld = opts.tld_suffix;

    let mut out = format!(
        "http:\n  routers:\n    traefik:\n      rule: \"Host(`traefik.{tld}`)\"\n      entryPoints:\n        - websecure\n      service: api@internal\n      tls: {{}}\n"
    );

    for (service, _) in TRAEFIK_ROUTED {
        if !opts.enabled(service) {
            continue;
        }
        let host = opts.subdomain(service);
        out.push_str(&format!(
            "    {service}:\n      rule: \"Host(`{host}.{tld}`)\"\n      entryPoints:\n        - websecure\n      service: {service}\n      tls: {{}}\n"
        ));
    }

    out.push_str("\n  services:\n");
    for (service, port) in TRAEFIK_ROUTED {
        if !opts.enabled(service) {
            continue;
        }
        out.push_str(&format!(
            "    {service}:\n      loadBalancer:\n        servers:\n          - url: \"http://stackvo-{service}:{port}\"\n"
        ));
    }

    if opts.ssl_enabled {
        out.push_str(
            "\n# TLS Configuration - Force use of core/certs certificates\ntls:\n  stores:\n    default:\n      defaultCertificate:\n        certFile: /certs/stackvo-wildcard.crt\n        keyFile: /certs/stackvo-wildcard.key\n  certificates:\n    - certFile: /certs/stackvo-wildcard.crt\n      keyFile: /certs/stackvo-wildcard.key\n  options:\n    default:\n      minVersion: VersionTLS12\n      sniStrict: false\n",
        );
    }

    out
}

/// Every router points at the `websecure` entry point unconditionally, but
/// `websecure` only exists when SSL is on — so with `SSL_ENABLE=false` the
/// generated pair is internally inconsistent and nothing is reachable.
///
/// Returned as a diagnostic rather than fixed: changing it would alter what the
/// Bash generator produces, and the differential tests exist to prove this port
/// does not do that. See CONFLICTS.md C-20.
pub fn traefik_routing_warning(opts: &TraefikOptions) -> Option<String> {
    (!opts.ssl_enabled).then(|| {
        "SSL_ENABLE is false, so no `websecure` entry point is generated — but every router still \
         targets it. No service domain will resolve until SSL is enabled."
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exts(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn builtins_contribute_neither_install_lines_nor_packages() {
        // `dom` maps to libxml2-dev in the matrix, but project.sh `continue`s
        // before collecting packages, so the real Dockerfiles have no
        // libxml2-dev. Matching that is the whole point.
        let plan = resolve("8.4", &exts(&["dom", "xml", "curl"]), false).unwrap();
        assert_eq!(plan.docker_ext, vec!["curl"]);
        assert_eq!(plan.apt_packages, vec!["libcurl4-openssl-dev"]);
        assert!(!plan.apt_packages.iter().any(|p| p == "libxml2-dev"));
    }

    #[test]
    fn apt_packages_are_sorted_and_deduplicated() {
        // pdo_pgsql and pgsql both pull libpq-dev.
        let plan = resolve(
            "8.4",
            &exts(&["zip", "mbstring", "pdo_pgsql", "pgsql"]),
            false,
        )
        .unwrap();
        assert_eq!(
            plan.apt_packages,
            vec!["libonig-dev", "libpq-dev", "libzip-dev"]
        );
    }

    #[test]
    fn extension_batches_follow_the_dependency_order() {
        let plan = resolve(
            "8.4",
            &exts(&["mbstring", "pdo", "curl", "pdo_mysql", "pdo_pgsql", "zip"]),
            false,
        )
        .unwrap();
        let block = extension_install_block(&plan);

        let lines: Vec<&str> = block.lines().filter(|l| l.starts_with("RUN")).collect();
        assert_eq!(lines[0], "RUN docker-php-ext-install pdo");
        assert_eq!(lines[1], "RUN docker-php-ext-install pdo_mysql pdo_pgsql");
        assert_eq!(lines[2], "RUN docker-php-ext-install mbstring curl zip");
    }

    #[test]
    fn pecl_dependencies_are_installed_before_their_dependents() {
        let plan = resolve("8.4", &exts(&["redis", "xdebug"]), false).unwrap();
        let names: Vec<&str> = plan.pecl.iter().map(|(e, _)| e.as_str()).collect();
        // redis pulls igbinary, which must land first.
        assert_eq!(names, vec!["igbinary", "redis", "xdebug"]);
    }

    #[test]
    fn pecl_versions_are_pinned_per_php_release() {
        let on_84 = resolve("8.4", &exts(&["redis"]), false).unwrap();
        assert_eq!(on_84.pecl[1].1.as_deref(), Some("6.3.0"));

        let on_81 = resolve("8.1", &exts(&["redis"]), false).unwrap();
        assert_eq!(on_81.pecl[1].1.as_deref(), Some("6.0.2"));
    }

    #[test]
    fn compat_mode_drops_imap_silently_and_strict_mode_refuses() {
        let compat = resolve("8.4", &exts(&["imap", "curl"]), false).unwrap();
        assert!(compat.docker_ext.iter().all(|e| e != "imap"));
        assert!(compat
            .skipped
            .iter()
            .any(|(e, r)| e == "imap" && r.contains("8.2")));

        // The v1 behaviour change: the same input is an error when strict.
        assert!(resolve("8.4", &exts(&["imap"]), true).is_err());
    }

    #[test]
    fn unknown_extensions_reach_docker_in_compat_mode_but_fail_strict() {
        // The Bash `*)` catch-all returns no packages, so a typo is passed
        // through to docker-php-ext-install and fails there.
        let compat = resolve("8.4", &exts(&["mbstirng"]), false).unwrap();
        assert!(compat.docker_ext.contains(&"mbstirng".to_string()));
        assert!(resolve("8.4", &exts(&["mbstirng"]), true).is_err());
    }
}
