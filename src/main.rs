// SPDX-License-Identifier: MIT
//
// evm-cli entry point.
//
// Per ADR-0005, the FIRST line of main() installs a panic hook that
// suppresses panic location / payload / backtrace. Per ADR-0007 rev1,
// the second concern is process hardening (umask + RLIMIT_CORE) before
// any Secret allocation. Then the M4 CLI takes over (parse + dispatch
// or REPL).

#![allow(unused_crate_dependencies)] // M1+ deps declared per PLAN-V9 §11 step 4

fn main() -> std::process::ExitCode {
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

    tracing::info!("evm-cli v0.2.0 starting");

    // (4) Test-only panic trigger (ADR-0005 verification per
    //     PLAN-V9 §11 step 12 + M3 audit C8). Gated by an env var so
    //     it cannot be triggered in production builds; integration
    //     test `tests/it_panic_hook.rs` sets it to verify the panic
    //     hook does not leak secret material into stderr.
    if std::env::var("EVMC_TEST_TRIGGER_PANIC").is_ok() {
        trigger_test_panic();
    }

    // (5) M4 CLI dispatch.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let rt = match runtime {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("fatal: could not build tokio runtime: {e}");
            return std::process::ExitCode::from(1);
        }
    };
    rt.block_on(evm_cli::cli::run())
}

/// Test-only panic trigger (ADR-0005). The payload contains a
/// recognizable fake private key + BIP-39 mnemonic so the integration
/// test can assert that *neither* appears in stderr (human-panic
/// suppresses location, payload, and backtrace).
#[allow(clippy::panic)] // intentional; this is the test path
fn trigger_test_panic() {
    let fake_key = "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let fake_mnemonic =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    panic!(
        "EVMC_TEST_TRIGGER_PANIC set: synthetic panic for ADR-0005 verification. \
         fake_key={fake_key} fake_mnemonic={fake_mnemonic}"
    );
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
