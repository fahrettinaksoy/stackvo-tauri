//! Stripe webhooks, forwarded to a project (M-11).
//!
//! Testing a payment flow means Stripe reaching the application, and
//! `shop.loc` does not exist on the internet. [`crate::tunnel`] answers the
//! general form of that, and for Stripe specifically it is the wrong tool:
//! a quick tunnel's URL changes on every start, so the webhook endpoint has to
//! be re-registered in the dashboard each time, and the signing secret changes
//! with it. Stripe's own CLI exists precisely to avoid that — `stripe listen`
//! opens an **outbound** connection, so nothing has to be reachable at all, and
//! it prints a signing secret that stays stable for the session.
//!
//! ## The API key does not go in a file
//!
//! `stripe listen` needs a restricted or secret key. This app already has an
//! answer for a credential — [`crate::secrets`] puts it in Keychain, Credential
//! Manager or the Secret Service — and this uses it rather than inventing a
//! second place. The key reaches the container as an environment variable on
//! `docker run`, which is the narrowest hand-off available: it is not in the
//! compose file, not in `.env`, and not in the image.
//!
//! `STRIPE_API_KEY` rather than `--api-key`: an argument is visible in
//! `docker inspect`, in `ps` output on the host, and in this app's own
//! operation console, which streams the command it ran.
//!
//! ## What this cannot verify on the machine it was written on
//!
//! Everything here except the account. The sidecar starts, the arguments are
//! the CLI's own, and the signing secret is read back out of the log — all of
//! that is measured in `examples/stripe_probe.rs`, which runs the real image
//! with a deliberately invalid key and checks that the CLI's own rejection
//! comes back through the log reader. **What no test here has is a Stripe
//! account**, so "a real event arrived at the application" is stated as
//! unverified rather than implied. The line between the two is written into
//! the probe's output.

use crate::error::{Code, Error, Result};
use serde::Serialize;

/// Sidecars are `stackvo-stripe-<project>`; the id handed to `engine::*`
/// (which prefixes `stackvo-` itself) is `stripe-<project>`.
pub const ID_PREFIX: &str = "stripe-";

pub const IMAGE: &str = "stripe/stripe-cli:latest";

/// The keystore entry the key is kept under, one per project.
///
/// Per project because a key is per Stripe account and somebody works on two.
/// A single shared entry would silently forward one account's events into
/// another account's application, which is the kind of wrong that looks like
/// it is working.
pub fn secret_name(project: &str) -> String {
    format!("stripe-api-key:{project}")
}

pub fn container_id(project: &str) -> String {
    format!("{ID_PREFIX}{project}")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StripeStatus {
    pub project: String,
    pub running: bool,
    pub container: String,
    /// The webhook signing secret the CLI printed, once it has. `None` while it
    /// is still connecting — the UI polls until it appears, the same way the
    /// tunnel's URL does.
    pub signing_secret: Option<String>,
    /// Whether a key is in the keystore for this project. Never the key.
    pub has_key: bool,
    /// The CLI's own complaint, when it failed. Read from the log rather than
    /// guessed at: "that key is not valid" and "there is no network" are
    /// different problems with different fixes and they look identical from
    /// the outside.
    pub failure: Option<String>,
}

/// The signing secret in a `stripe listen` log.
///
/// The CLI prints `Ready! You are using Stripe API Version [...]. Your webhook
/// signing secret is whsec_… (^C to quit)`. The prefix is the stable part —
/// the sentence around it has been reworded across releases.
pub fn find_secret(log: &str) -> Option<String> {
    for line in log.lines() {
        let Some(start) = line.find("whsec_") else {
            continue;
        };
        let secret: String = line[start..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        // `whsec_` alone is the prefix with nothing after it, which is a
        // sentence about the secret rather than the secret.
        if secret.len() > 6 {
            return Some(secret);
        }
    }
    None
}

/// The CLI's own failure line, if it printed one.
///
/// Matched on what the CLI says rather than on an exit code: the container is
/// long-running, so by the time anything is read it has usually not exited at
/// all — it is sitting there having failed.
pub fn find_failure(log: &str) -> Option<String> {
    for line in log.lines() {
        let lowered = line.to_ascii_lowercase();
        if lowered.contains("invalid api key")
            || lowered.contains("authentication")
            || lowered.contains("you have not configured")
            || lowered.starts_with("error")
            || lowered.contains("fatal")
        {
            return Some(line.trim().to_string());
        }
    }
    None
}

/// Where inside the project container the events are forwarded.
///
/// Checked rather than accepted: this is a path in somebody else's application
/// and it goes into a URL that a container resolves. A space or a scheme in it
/// produces a `--forward-to` the CLI rejects with a message about a flag rather
/// than about the field somebody typed in.
pub fn checked_path(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.contains(char::is_whitespace) || trimmed.contains("://") {
        return Err(Error::new(
            Code::InvalidInput,
            "the forward path is a path inside the application, not a URL",
        ));
    }
    Ok(format!("/{}", trimmed.trim_start_matches('/')))
}

/// The `docker run` invocation for one project's listener.
///
/// Returned as arguments rather than executed, like the tunnel's: the first
/// start pulls the image and that belongs in the operation console.
///
/// The key is NOT among them. It is passed with `-e STRIPE_API_KEY` and the
/// value comes from the process environment of the spawned command, so the
/// argv this returns can be printed into a console without printing a
/// credential.
pub fn run_args(
    project: &str,
    port: u16,
    path: &str,
    events: &[String],
    network: &str,
) -> Vec<String> {
    let target = format!(
        "http://{}:{port}{path}",
        crate::engine::container_name(project)
    );

    let mut args: Vec<String> = [
        "run",
        "-d",
        // No `--rm`, and that is a correction rather than an omission. The
        // tunnel's sidecar uses it, and copying that here produced a real
        // defect: an invalid key makes the CLI print its complaint and exit,
        // `--rm` takes the container away with the log, and `status_all` then
        // finds nothing at all — so the pane showed no listener, no error and
        // no reason, for the single most likely failure this feature has. A
        // stopped container that still holds its log is what makes "that key
        // was rejected" sayable. `stripe_stop` removes it explicitly, and
        // `stripe_start` clears the old one before it starts a new one.
        "--name",
        &format!("stackvo-{}", container_id(project)),
        "--network",
        network,
        "-e",
        "STRIPE_API_KEY",
        IMAGE,
        "listen",
        "--forward-to",
        &target,
        // Without it the CLI asks whether to set up a webhook endpoint and
        // waits for an answer nobody can give it inside a detached container.
        "--skip-verify",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    // An empty list means every event, which is the CLI's own default and the
    // right one for "does anything arrive at all".
    if !events.is_empty() {
        args.push("--events".into());
        args.push(events.join(","));
    }
    args
}

/// Every listener sidecar, with whatever its log says about it.
///
/// The secret and the failure are read from the log on every call rather than
/// cached, for the reason the tunnel's URL is: what the log says is what is
/// actually live, across app restarts and container crashes alike.
pub async fn status_all() -> Result<Vec<StripeStatus>> {
    use futures_util::StreamExt;

    let containers = crate::engine::stackvo_containers().await?;
    let mut out = Vec::new();

    for (id, info) in containers {
        let Some(project) = id.strip_prefix(ID_PREFIX) else {
            continue;
        };

        let (signing_secret, failure) = if info.running {
            match crate::engine::logs_stream(&id, 200, false) {
                Ok(stream) => {
                    let lines: Vec<String> = stream.map(|l| l.text).collect().await;
                    let log = lines.join("\n");
                    (find_secret(&log), find_failure(&log))
                }
                Err(_) => (None, None),
            }
        } else {
            (None, None)
        };

        out.push(StripeStatus {
            has_key: crate::secrets::read(&secret_name(project))
                .ok()
                .flatten()
                .is_some(),
            project: project.to_string(),
            running: info.running,
            container: info.name,
            signing_secret,
            failure,
        });
    }

    out.sort_by(|a, b| a.project.cmp(&b.project));
    Ok(out)
}

/// Refuse to forward to a container that is not running.
///
/// Without this the CLI accepts the events from Stripe, fails to deliver each
/// one, and reports the failures back to Stripe — so the dashboard fills with
/// delivery errors for a project that was simply not started.
pub async fn ensure_project_running(project: &str) -> Result<()> {
    let containers = crate::engine::stackvo_containers().await?;
    match containers.get(project) {
        Some(info) if info.running => Ok(()),
        Some(_) => Err(Error::new(
            Code::Conflict,
            format!("{project} is not running"),
        )),
        None => Err(Error::not_found(format!("container for {project}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The line the CLI actually prints, with the wording it used when this
    /// was written — matched on the prefix, which has not changed across the
    /// rewordings of the sentence around it.
    #[test]
    fn the_signing_secret_is_read_out_of_the_log() {
        let log = "\
Getting ready...\n\
> Ready! You are using Stripe API Version [2025-08-27]. Your webhook signing secret is whsec_1a2b3c4d5e6f7g8h9i0j (^C to quit)\n";
        assert_eq!(
            find_secret(log).as_deref(),
            Some("whsec_1a2b3c4d5e6f7g8h9i0j")
        );
        assert_eq!(find_secret("nothing here"), None);
        // The word without a secret after it is a sentence, not an answer.
        assert_eq!(find_secret("the whsec_ is printed on start"), None);
    }

    /// A failed listener sits there rather than exiting, so the log is the only
    /// place the reason exists.
    #[test]
    fn the_reason_it_failed_is_read_out_of_the_log() {
        assert!(find_failure("Invalid API Key provided: sk_test_***").is_some());
        assert!(find_failure("FATAL could not reach api.stripe.com").is_some());
        assert_eq!(find_failure("Getting ready...\n"), None);
    }

    /// The key must not be an argument: argv reaches `docker inspect`, the
    /// host's process list and this app's own operation console, which streams
    /// the command it ran.
    #[test]
    fn the_api_key_is_never_an_argument() {
        let args = run_args("shop", 80, "/stripe/webhook", &[], "stackvo-net");
        let joined = args.join(" ");
        assert!(joined.contains("-e STRIPE_API_KEY"));
        assert!(
            !joined.contains("sk_") && !joined.contains("--api-key"),
            "a credential reached the argument list"
        );
        assert!(joined.contains("--forward-to http://stackvo-shop:80/stripe/webhook"));
        // Deliberately NOT `--rm`: a listener that fails exits, and a removed
        // container takes the reason with it. See the note beside the flag.
        assert!(!joined.contains("--rm"));
    }

    #[test]
    fn events_are_only_narrowed_when_asked_for() {
        let all = run_args("shop", 80, "/hook", &[], "net");
        assert!(!all.contains(&"--events".to_string()));

        let some = run_args(
            "shop",
            80,
            "/hook",
            &["payment_intent.succeeded".into(), "charge.refunded".into()],
            "net",
        );
        let at = some.iter().position(|a| a == "--events").unwrap();
        assert_eq!(some[at + 1], "payment_intent.succeeded,charge.refunded");
    }

    #[test]
    fn a_forward_path_is_a_path_and_not_a_url() {
        assert_eq!(checked_path("stripe/webhook").unwrap(), "/stripe/webhook");
        assert_eq!(checked_path("/stripe/webhook").unwrap(), "/stripe/webhook");
        assert!(checked_path("https://shop.loc/hook").is_err());
        assert!(checked_path("/two words").is_err());
    }

    /// One entry per project. A single shared one would forward one account's
    /// events into another account's application, which looks like it works.
    #[test]
    fn the_keystore_entry_is_per_project() {
        assert_eq!(secret_name("shop"), "stripe-api-key:shop");
        assert_ne!(secret_name("shop"), secret_name("blog"));
    }
}
