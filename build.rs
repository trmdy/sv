use std::env;
use std::process::Command;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "macos" {
        return;
    }

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if target_arch != "x86_64" {
        return;
    }

    let uname_arch = match uname_arch() {
        Some(arch) => arch,
        None => return,
    };

    if uname_arch != "arm64" && uname_arch != "aarch64" {
        return;
    }

    let openssl_keys = ["OPENSSL_DIR", "OPENSSL_LIB_DIR", "OPENSSL_INCLUDE_DIR"];
    let env_keys = [
        "OPENSSL_DIR",
        "OPENSSL_LIB_DIR",
        "OPENSSL_INCLUDE_DIR",
        "PKG_CONFIG_PATH",
        "LDFLAGS",
        "CPPFLAGS",
        "LIBRARY_PATH",
    ];

    for key in env_keys {
        println!("cargo:rerun-if-env-changed={}", key);
    }

    if has_explicit_x86_openssl(&openssl_keys) {
        return;
    }

    let mut bad_keys = Vec::new();
    for key in env_keys {
        if env::var(key)
            .map(|value| value.contains("/opt/homebrew"))
            .unwrap_or(false)
        {
            bad_keys.push(key);
        }
    }

    let pkg_config_bad = pkg_config_uses_opt_homebrew();
    if bad_keys.is_empty() && !pkg_config_bad {
        return;
    }

    let mut message = String::from(
        "Detected Apple Silicon host (arm64) building x86_64 with Homebrew OpenSSL from /opt/homebrew.\n",
    );
    message.push_str(
        "This usually links arm64 OpenSSL and fails with unresolved _OPENSSL_init_ssl.\n",
    );
    message.push_str("Fix:\n");
    message.push_str("  - Use an arm64 toolchain: rustup default stable-aarch64-apple-darwin\n");
    message.push_str(
        "  - OR use x86_64 Homebrew under /usr/local and set OPENSSL_DIR/PKG_CONFIG_PATH/LDFLAGS/CPPFLAGS accordingly.\n",
    );
    message.push_str(
        "If you intentionally cross-compile, ensure your OpenSSL env points to x86_64 libs.\n",
    );
    if !bad_keys.is_empty() {
        message.push_str("Found /opt/homebrew in: ");
        message.push_str(&bad_keys.join(", "));
        message.push('\n');
    }
    if pkg_config_bad {
        message.push_str("pkg-config reports /opt/homebrew OpenSSL.\n");
    }

    panic!("{}", message);
}

fn uname_arch() -> Option<String> {
    let output = Command::new("uname").arg("-m").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let arch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if arch.is_empty() {
        None
    } else {
        Some(arch)
    }
}

fn has_explicit_x86_openssl(openssl_keys: &[&str]) -> bool {
    for key in openssl_keys {
        let value = match env::var(key) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if value.contains("/usr/local") || value.contains("/opt/local") {
            return true;
        }
    }
    false
}

fn pkg_config_uses_opt_homebrew() -> bool {
    let output = match Command::new("pkg-config")
        .arg("--libs")
        .arg("openssl")
        .output()
    {
        Ok(output) => output,
        Err(_) => return false,
    };

    if !output.status.success() {
        return false;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    text.contains("/opt/homebrew")
}
