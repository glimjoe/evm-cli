// SPDX-License-Identifier: MIT
//
// evm-cli M0 entry point.
//
// Per ADR-0005, the FIRST line of main() installs a panic hook that
// suppresses panic location / payload / backtrace. Per ADR-0007 rev1,
// the second concern is process hardening (umask + RLIMIT_CORE) before
// any Secret allocation.

#![allow(unused_crate_dependencies)] // M1+ deps declared per V8 §11 step 4

fn main() {
    // (1) Panic hook — FIRST line, before any allocation. ADR-0005.
    //    Note: human-panic 2.x exposes `setup_panic!()`, not `setup!()`.
    human_panic::setup_panic!();

    // (2) Process hardening — before any Secret allocation. ADR-0007 rev1.
    if let Err(e) = harden_process() {
        eprintln!("warning: process hardening failed: {e}");
    }

    // (3) Observability. M0 placeholder; M4 wires up EnvFilter properly.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,evm_cli=debug")),
        )
        .with_target(false)
        .init();

    tracing::info!("evm-cli v0.1.0 starting (M0 scaffolding)");

    // (4) M0 banner. M4+ replaces this with the REPL.
    println!("evm-cli v0.1.0 — M0 scaffolding complete.");
    println!("No commands implemented yet; this is a smoke test for the");
    println!("panic hook, process hardening, and tracing pipeline.");
    println!();
    println!("Planned M1+ commands (per V8 §5):");
    println!("  create-wallet, import-mnemonic, list, use, unlock,");
    println!("  balance, send-eth, send-token, sign-message, pending-tx, exit");
}

/// Process hardening per ADR-0007 rev1 (Decision Outcome §2).
///
/// Must run before any `Secret<Vec<u8>>` allocation. Failures are
/// non-fatal (logged at WARN) but indicate a degraded security posture
/// (e.g. core dumps may be enabled, or files may be created world-readable).
fn harden_process() -> Result<(), String> {
    // (a) Disable core dumps: RLIMIT_CORE = 0
    let rlim = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: rlim is a POD initialized above; the kernel validates the pointer.
    let rc = unsafe { libc::setrlimit(libc::RLIMIT_CORE, &rlim) };
    if rc != 0 {
        return Err(format!(
            "setrlimit(RLIMIT_CORE) failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    // (b) Restrict file mode for any file we create: umask(0o077)
    // SAFETY: umask is always safe to call.
    unsafe { libc::umask(0o077) };

    Ok(())
}
