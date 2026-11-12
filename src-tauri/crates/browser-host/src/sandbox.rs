//! Whether the engine can run inside Chromium's sandbox on this computer. The sandbox is what keeps
//! a web page that exploits a flaw in the renderer from reaching the user's files, so it is on
//! whenever the system allows it, and the host says so when it has to go without.

/// `1` turns the sandbox off even where it would work, for diagnosing the engine.
pub const NO_SANDBOX_ENV: &str = "LYNX_ENGINE_NO_SANDBOX";

/// The sandbox's state for this run, and why it is off when it is.
pub struct Sandbox {
    pub enabled: bool,
    pub reason: &'static str,
}

pub fn decide() -> Sandbox {
    if std::env::var(NO_SANDBOX_ENV).as_deref() == Ok("1") {
        return Sandbox {
            enabled: false,
            reason: "turned off by LYNX_ENGINE_NO_SANDBOX",
        };
    }
    platform::decide()
}

#[cfg(target_os = "linux")]
mod platform {
    use super::Sandbox;

    /// Chromium's sandbox on Linux builds its jail from user namespaces. Some systems switch them
    /// off, and Ubuntu 24.04 and later let a program create one but take its powers away unless an
    /// AppArmor profile says otherwise, which leaves the sandbox unable to start.
    pub fn decide() -> Sandbox {
        if restricted_by_apparmor() {
            return off("the system restricts unprivileged user namespaces (AppArmor)");
        }
        if !can_create_user_namespace() {
            return off("the system does not allow unprivileged user namespaces");
        }
        Sandbox {
            enabled: true,
            reason: "",
        }
    }

    fn off(reason: &'static str) -> Sandbox {
        Sandbox {
            enabled: false,
            reason,
        }
    }

    fn restricted_by_apparmor() -> bool {
        std::fs::read_to_string("/proc/sys/kernel/apparmor_restrict_unprivileged_userns")
            .is_ok_and(|text| text.trim() == "1")
    }

    /// Tries it in a child process: `unshare` for a user namespace only works in a program with a
    /// single thread, and the host already has more than one.
    fn can_create_user_namespace() -> bool {
        // SAFETY: the child only calls `unshare` and `_exit`, which are safe after `fork`.
        unsafe {
            match libc::fork() {
                -1 => false,
                0 => {
                    let code = i32::from(libc::unshare(libc::CLONE_NEWUSER) != 0);
                    libc::_exit(code)
                }
                child => {
                    let mut status = 0;
                    libc::waitpid(child, &mut status, 0) == child
                        && libc::WIFEXITED(status)
                        && libc::WEXITSTATUS(status) == 0
                }
            }
        }
    }
}

// Windows and macOS: the sandbox needs steps this host does not take yet, so the engine runs
// without it there.
#[cfg(not(target_os = "linux"))]
mod platform {
    use super::Sandbox;

    pub fn decide() -> Sandbox {
        Sandbox {
            enabled: false,
            reason: "the sandbox is not set up on this platform yet",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sandbox_that_is_off_says_why() {
        let sandbox = decide();
        assert!(sandbox.enabled || !sandbox.reason.is_empty());
    }
}
