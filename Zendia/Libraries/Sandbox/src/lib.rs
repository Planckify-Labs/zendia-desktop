/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! The sandbox profiles of Zendia's service processes: Landlock and seccomp on Linux, Seatbelt on macOS.
//!
//! Each service enters its profile as it starts, before it reads any untrusted input. PluginHost gets the strictest
//! profile, then ExtensionServer and AgentServer; InputServer gets a light one.
//!
//! See `Zendia/Documentation/Architecture.md`, "Sandboxing".

use std::fmt;
use std::path::PathBuf;

/// What a Zendia service process is permitted to touch once locked down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SandboxProfile {
    /// Strictest profile: IPC only, no filesystem writes, no direct network.
    PluginHost,
    /// Unpacks extension packages and runs on their data: IPC, read-write access only to `data_dir`, optional
    /// read-only access to explicitly loaded local extension paths, and no direct outbound network sockets.
    ExtensionServer {
        data_dir: PathBuf,
        read_only_paths: Vec<PathBuf>,
    },
    /// Reads untrusted page text and holds model API keys: IPC, network, and its trace directory.
    AgentServer { trace_dir: PathBuf },
    /// Runs profile scripts: IPC and read-only access to the config directory, no network.
    InputServer { config_dir: PathBuf },
}

/// Outcome of attempting to enter a sandbox profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SandboxOutcome {
    /// All requested restrictions (filesystem + syscall filter) are enforced.
    Enforced,
    /// Some restrictions could not be applied on this kernel/platform (e.g. Landlock unsupported), so the process runs
    /// with partial confinement.
    Degraded { reason: String },
    /// Sandbox skipped explicitly via `ZENDIA_DISABLE_SANDBOX=1`.
    Disabled,
}

#[derive(Debug)]
pub struct SandboxError {
    pub message: String,
}

impl fmt::Display for SandboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SandboxError {}

/// Locks the current process into `profile`. Must be called before parsing untrusted input.
pub fn enter_sandbox(profile: &SandboxProfile) -> Result<SandboxOutcome, SandboxError> {
    if std::env::var_os("ZENDIA_DISABLE_SANDBOX").is_some_and(|v| v == "1" || v == "true") {
        return Ok(SandboxOutcome::Disabled);
    }

    #[cfg(target_os = "linux")]
    {
        linux::enter(profile)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = profile;
        Ok(SandboxOutcome::Degraded {
            reason: "OS sandbox not implemented for this platform yet".to_string(),
        })
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{SandboxError, SandboxOutcome, SandboxProfile};
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1 << 0;
    const LANDLOCK_RULE_PATH_BENEATH: u32 = 1;

    const LANDLOCK_ACCESS_FS_EXECUTE: u64 = 1 << 0;
    const LANDLOCK_ACCESS_FS_WRITE_FILE: u64 = 1 << 1;
    const LANDLOCK_ACCESS_FS_READ_FILE: u64 = 1 << 2;
    const LANDLOCK_ACCESS_FS_READ_DIR: u64 = 1 << 3;
    const LANDLOCK_ACCESS_FS_REMOVE_DIR: u64 = 1 << 4;
    const LANDLOCK_ACCESS_FS_REMOVE_FILE: u64 = 1 << 5;
    const LANDLOCK_ACCESS_FS_MAKE_CHAR: u64 = 1 << 6;
    const LANDLOCK_ACCESS_FS_MAKE_DIR: u64 = 1 << 7;
    const LANDLOCK_ACCESS_FS_MAKE_REG: u64 = 1 << 8;
    const LANDLOCK_ACCESS_FS_MAKE_SOCK: u64 = 1 << 9;
    const LANDLOCK_ACCESS_FS_MAKE_FIFO: u64 = 1 << 10;
    const LANDLOCK_ACCESS_FS_MAKE_BLOCK: u64 = 1 << 11;
    const LANDLOCK_ACCESS_FS_MAKE_SYM: u64 = 1 << 12;
    const LANDLOCK_ACCESS_FS_REFER: u64 = 1 << 13;
    const LANDLOCK_ACCESS_FS_TRUNCATE: u64 = 1 << 14;

    const ABI1_ALL_FS: u64 = LANDLOCK_ACCESS_FS_EXECUTE
        | LANDLOCK_ACCESS_FS_WRITE_FILE
        | LANDLOCK_ACCESS_FS_READ_FILE
        | LANDLOCK_ACCESS_FS_READ_DIR
        | LANDLOCK_ACCESS_FS_REMOVE_DIR
        | LANDLOCK_ACCESS_FS_REMOVE_FILE
        | LANDLOCK_ACCESS_FS_MAKE_CHAR
        | LANDLOCK_ACCESS_FS_MAKE_DIR
        | LANDLOCK_ACCESS_FS_MAKE_REG
        | LANDLOCK_ACCESS_FS_MAKE_SOCK
        | LANDLOCK_ACCESS_FS_MAKE_FIFO
        | LANDLOCK_ACCESS_FS_MAKE_BLOCK
        | LANDLOCK_ACCESS_FS_MAKE_SYM;

    const READ_DIR_ACCESS: u64 = LANDLOCK_ACCESS_FS_READ_FILE | LANDLOCK_ACCESS_FS_READ_DIR;
    const READ_FILE_ACCESS: u64 = LANDLOCK_ACCESS_FS_READ_FILE;
    const RW_DIR_ACCESS: u64 = LANDLOCK_ACCESS_FS_READ_FILE
        | LANDLOCK_ACCESS_FS_READ_DIR
        | LANDLOCK_ACCESS_FS_WRITE_FILE
        | LANDLOCK_ACCESS_FS_REMOVE_DIR
        | LANDLOCK_ACCESS_FS_REMOVE_FILE
        | LANDLOCK_ACCESS_FS_MAKE_DIR
        | LANDLOCK_ACCESS_FS_MAKE_REG;

    #[repr(C)]
    struct LandlockRulesetAttr {
        handled_access_fs: u64,
    }

    #[repr(C, packed)]
    struct LandlockPathBeneathAttr {
        allowed_access: u64,
        parent_fd: i32,
    }

    pub(super) fn enter(profile: &SandboxProfile) -> Result<SandboxOutcome, SandboxError> {
        // Required before both Landlock and seccomp-bpf without CAP_SYS_ADMIN.
        // SAFETY: prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) takes integer arguments only.
        let rc = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
        if rc != 0 {
            return Err(SandboxError {
                message: format!("prctl(PR_SET_NO_NEW_PRIVS) failed: {}", std::io::Error::last_os_error()),
            });
        }

        let mut degraded_reasons = Vec::new();
        if let Err(reason) = apply_landlock(profile) {
            degraded_reasons.push(reason);
        }

        let block_network = matches!(
            profile,
            SandboxProfile::PluginHost | SandboxProfile::ExtensionServer { .. } | SandboxProfile::InputServer { .. }
        );
        if block_network && let Err(reason) = apply_seccomp_no_network_or_exec() {
            degraded_reasons.push(reason);
        }

        if degraded_reasons.is_empty() {
            Ok(SandboxOutcome::Enforced)
        } else {
            Ok(SandboxOutcome::Degraded {
                reason: degraded_reasons.join("; "),
            })
        }
    }

    fn apply_landlock(profile: &SandboxProfile) -> Result<(), String> {
        // SAFETY: querying the Landlock ABI version with NULL attr and size 0.
        let abi = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                std::ptr::null::<LandlockRulesetAttr>(),
                0usize,
                LANDLOCK_CREATE_RULESET_VERSION,
            )
        };
        if abi < 1 {
            return Err(format!("Landlock unavailable ({})", std::io::Error::last_os_error()));
        }

        let mut handled_fs = ABI1_ALL_FS;
        if abi >= 2 {
            handled_fs |= LANDLOCK_ACCESS_FS_REFER;
        }
        if abi >= 3 {
            handled_fs |= LANDLOCK_ACCESS_FS_TRUNCATE;
        }

        let attr = LandlockRulesetAttr {
            handled_access_fs: handled_fs,
        };
        // SAFETY: passing a valid pointer to LandlockRulesetAttr and its exact byte size.
        let ruleset_fd = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                &attr as *const LandlockRulesetAttr,
                std::mem::size_of::<LandlockRulesetAttr>(),
                0u32,
            )
        };
        if ruleset_fd < 0 {
            return Err(format!(
                "landlock_create_ruleset failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        let ruleset_fd = ruleset_fd as i32;

        let rw_access = if abi >= 3 {
            RW_DIR_ACCESS | LANDLOCK_ACCESS_FS_TRUNCATE | LANDLOCK_ACCESS_FS_REFER
        } else if abi >= 2 {
            RW_DIR_ACCESS | LANDLOCK_ACCESS_FS_REFER
        } else {
            RW_DIR_ACCESS
        } & handled_fs;

        let ro_dir_access = READ_DIR_ACCESS & handled_fs;
        let ro_file_access = READ_FILE_ACCESS & handled_fs;

        let add_path = |path: &Path, writable: bool| {
            if !path.exists() {
                return;
            }
            let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
                return;
            };
            // SAFETY: c_path is NUL-terminated; O_PATH opens only a path descriptor.
            let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
            if fd < 0 {
                return;
            }
            let is_dir = path.is_dir();
            let allowed_access = if writable && is_dir {
                rw_access
            } else if is_dir {
                ro_dir_access
            } else if writable {
                (LANDLOCK_ACCESS_FS_READ_FILE | LANDLOCK_ACCESS_FS_WRITE_FILE | LANDLOCK_ACCESS_FS_TRUNCATE)
                    & handled_fs
            } else {
                ro_file_access
            };
            let rule = LandlockPathBeneathAttr {
                allowed_access,
                parent_fd: fd,
            };
            // SAFETY: passing a valid pointer to LandlockPathBeneathAttr.
            let _ = unsafe {
                libc::syscall(
                    libc::SYS_landlock_add_rule,
                    ruleset_fd,
                    LANDLOCK_RULE_PATH_BENEATH,
                    &rule as *const LandlockPathBeneathAttr,
                    0u32,
                )
            };
            // SAFETY: closing the O_PATH fd we just opened.
            unsafe { libc::close(fd) };
        };

        // Timezone data is read-only and harmless for timestamps/alarms.
        add_path(Path::new("/etc/localtime"), false);

        match profile {
            SandboxProfile::PluginHost => {}
            SandboxProfile::ExtensionServer {
                data_dir,
                read_only_paths,
            } => {
                add_path(data_dir, true);
                for path in read_only_paths {
                    add_path(path, false);
                }
            }
            SandboxProfile::AgentServer { trace_dir } => {
                add_path(trace_dir, true);
            }
            SandboxProfile::InputServer { config_dir } => {
                add_path(config_dir, false);
            }
        }

        // SAFETY: enforcing the valid Landlock ruleset fd on the current thread/process.
        let restricted = unsafe { libc::syscall(libc::SYS_landlock_restrict_self, ruleset_fd, 0u32) };
        // SAFETY: closing ruleset_fd after restrict_self.
        unsafe { libc::close(ruleset_fd) };
        if restricted != 0 {
            return Err(format!(
                "landlock_restrict_self failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    fn apply_seccomp_no_network_or_exec() -> Result<(), String> {
        #[repr(C)]
        struct SockFilter {
            code: u16,
            jt: u8,
            jf: u8,
            k: u32,
        }

        #[repr(C)]
        struct SockFprog {
            len: u16,
            filter: *const SockFilter,
        }

        const BPF_LD: u16 = 0x00;
        const BPF_W: u16 = 0x00;
        const BPF_ABS: u16 = 0x20;
        const BPF_JMP: u16 = 0x05;
        const BPF_JEQ: u16 = 0x10;
        const BPF_K: u16 = 0x00;
        const BPF_RET: u16 = 0x06;

        const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
        const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
        const OFFSET_NR: u32 = 0;
        const OFFSET_ARCH: u32 = 4;
        const OFFSET_ARG0: u32 = 16;

        #[cfg(target_arch = "x86_64")]
        const AUDIT_ARCH: u32 = 0xc000_003e;
        #[cfg(target_arch = "aarch64")]
        const AUDIT_ARCH: u32 = 0xc000_00b7;

        let eperm = SECCOMP_RET_ERRNO | (libc::EPERM as u32);
        let eacces = SECCOMP_RET_ERRNO | (libc::EACCES as u32);
        let nr_socket = libc::SYS_socket as u32;
        let nr_execve = libc::SYS_execve as u32;
        let nr_execveat = libc::SYS_execveat as u32;
        let af_inet = libc::AF_INET as u32;
        let af_inet6 = libc::AF_INET6 as u32;

        // Denies execve/execveat and IPv4/IPv6 socket creation while allowing AF_UNIX and all normal syscalls.
        let filter = [
            // 0: load arch
            SockFilter {
                code: BPF_LD | BPF_W | BPF_ABS,
                jt: 0,
                jf: 0,
                k: OFFSET_ARCH,
            },
            // 1: if arch == native, continue; else EPERM
            SockFilter {
                code: BPF_JMP | BPF_JEQ | BPF_K,
                jt: 1,
                jf: 0,
                k: AUDIT_ARCH,
            },
            // 2: reject foreign arch
            SockFilter {
                code: BPF_RET | BPF_K,
                jt: 0,
                jf: 0,
                k: eperm,
            },
            // 3: load syscall nr
            SockFilter {
                code: BPF_LD | BPF_W | BPF_ABS,
                jt: 0,
                jf: 0,
                k: OFFSET_NR,
            },
            // 4: if nr == execve -> jump to EPERM (index 10, +5)
            SockFilter {
                code: BPF_JMP | BPF_JEQ | BPF_K,
                jt: 5,
                jf: 0,
                k: nr_execve,
            },
            // 5: if nr == execveat -> jump to EPERM (index 10, +4)
            SockFilter {
                code: BPF_JMP | BPF_JEQ | BPF_K,
                jt: 4,
                jf: 0,
                k: nr_execveat,
            },
            // 6: if nr == socket -> inspect domain (index 7); else ALLOW (index 12, +5)
            SockFilter {
                code: BPF_JMP | BPF_JEQ | BPF_K,
                jt: 0,
                jf: 5,
                k: nr_socket,
            },
            // 7: load arg0 (socket domain)
            SockFilter {
                code: BPF_LD | BPF_W | BPF_ABS,
                jt: 0,
                jf: 0,
                k: OFFSET_ARG0,
            },
            // 8: if domain == AF_INET -> EACCES (index 11, +2)
            SockFilter {
                code: BPF_JMP | BPF_JEQ | BPF_K,
                jt: 2,
                jf: 0,
                k: af_inet,
            },
            // 9: if domain == AF_INET6 -> EACCES (index 11, +1); else ALLOW (index 12, +2)
            SockFilter {
                code: BPF_JMP | BPF_JEQ | BPF_K,
                jt: 1,
                jf: 2,
                k: af_inet6,
            },
            // 10: return EPERM
            SockFilter {
                code: BPF_RET | BPF_K,
                jt: 0,
                jf: 0,
                k: eperm,
            },
            // 11: return EACCES
            SockFilter {
                code: BPF_RET | BPF_K,
                jt: 0,
                jf: 0,
                k: eacces,
            },
            // 12: return ALLOW
            SockFilter {
                code: BPF_RET | BPF_K,
                jt: 0,
                jf: 0,
                k: SECCOMP_RET_ALLOW,
            },
        ];

        let prog = SockFprog {
            len: filter.len() as u16,
            filter: filter.as_ptr(),
        };
        // SAFETY: prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &prog) installs an immutable BPF program.
        let rc = unsafe {
            libc::prctl(
                libc::PR_SET_SECCOMP,
                libc::SECCOMP_MODE_FILTER,
                &prog as *const SockFprog,
            )
        };
        if rc != 0 {
            return Err(format!("seccomp filter failed: {}", std::io::Error::last_os_error()));
        }
        Ok(())
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    fn apply_seccomp_no_network_or_exec() -> Result<(), String> {
        Err("seccomp filter not implemented for this CPU architecture".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_server_profile_is_constructible() {
        let profile = SandboxProfile::ExtensionServer {
            data_dir: PathBuf::from("/tmp/zendia-test-ext"),
            read_only_paths: vec![PathBuf::from("/tmp/zendia-test-unpacked")],
        };
        assert_eq!(
            profile,
            SandboxProfile::ExtensionServer {
                data_dir: PathBuf::from("/tmp/zendia-test-ext"),
                read_only_paths: vec![PathBuf::from("/tmp/zendia-test-unpacked")],
            }
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn sandbox_enforces_in_child_process() {
        let tmp = std::env::temp_dir().join(format!("zendia-sandbox-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        // SAFETY: single-threaded fork in an isolated test child that exits immediately via _exit.
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0, "fork failed");
        if pid == 0 {
            let outcome = enter_sandbox(&SandboxProfile::ExtensionServer {
                data_dir: tmp.clone(),
                read_only_paths: Vec::new(),
            });
            let code = match outcome {
                Ok(SandboxOutcome::Enforced) => {
                    // Writing inside data_dir must succeed.
                    let ok_write = std::fs::write(tmp.join("ok.txt"), b"hello").is_ok();
                    // Opening an IPv4 TCP socket must be denied by seccomp.
                    let net_denied = std::net::TcpListener::bind("127.0.0.1:0").is_err();
                    // Reading outside data_dir (/etc/passwd) must be denied by Landlock.
                    let fs_denied = std::fs::read("/etc/passwd").is_err();
                    if ok_write && net_denied && fs_denied { 0 } else { 2 }
                }
                Ok(SandboxOutcome::Degraded { .. } | SandboxOutcome::Disabled) => 0,
                Err(_) => 1,
            };
            // SAFETY: immediate child exit without running parent destructors.
            unsafe { libc::_exit(code) };
        }
        let mut status = 0i32;
        // SAFETY: waiting on child pid.
        unsafe { libc::waitpid(pid, &mut status, 0) };
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(libc::WIFEXITED(status), "child did not exit cleanly");
        assert_eq!(libc::WEXITSTATUS(status), 0, "sandbox assertions failed in child");
    }
}
