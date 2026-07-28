//! What the gate would say on this machine, without opening a window.
//!
//!   cargo run --example preflight_report

#[tokio::main]
async fn main() {
    let result = stackvo_desktop_lib::preflight::run().await;
    println!("os = {} · ready = {}", result.os, result.ready);
    for r in &result.requirements {
        println!(
            "  {:<10} {:?}{}{}",
            r.id,
            r.state,
            r.detail.as_deref().map(|d| format!("  — {d}")).unwrap_or_default(),
            if r.fixable { "  [düzeltilebilir]" } else { "" }
        );
    }
}
