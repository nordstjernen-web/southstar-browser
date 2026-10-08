//! Southstar — startup security: refusing root or elevation, the Linux Landlock + seccomp sandbox, macOS Seatbelt, Windows mitigations, SRI checks and download marking.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SriDigest {
    Sha256,
    Sha384,
    Sha512,
}

impl SriDigest {
    fn prefix(self) -> &'static [u8] {
        match self {
            SriDigest::Sha256 => b"sha256-",
            SriDigest::Sha384 => b"sha384-",
            SriDigest::Sha512 => b"sha512-",
        }
    }
}

pub fn sri_check(integrity: &[u8], body: Option<&[u8]>) -> bool {
    if integrity.is_empty() {
        return true;
    }
    let Some(body) = body.filter(|body| !body.is_empty()) else {
        return false;
    };
    let tokens = || {
        integrity
            .split(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
            .filter(|token| !token.is_empty())
    };
    let strongest = tokens()
        .filter_map(|token| {
            [SriDigest::Sha512, SriDigest::Sha384, SriDigest::Sha256]
                .into_iter()
                .find(|digest| token.starts_with(digest.prefix()))
        })
        .max_by_key(|digest| match digest {
            SriDigest::Sha256 => 256,
            SriDigest::Sha384 => 384,
            SriDigest::Sha512 => 512,
        });
    let Some(digest) = strongest else {
        return true;
    };
    let mut expected: Option<Vec<u8>> = None;
    for token in tokens() {
        let Some(b64) = token.strip_prefix(digest.prefix()) else {
            continue;
        };
        let b64 = b64.split(|&byte| byte == b'?').next().unwrap_or_default();
        if b64.is_empty() {
            continue;
        }
        let got = expected.get_or_insert_with(|| ffi::digest_base64(digest, body));
        if got.as_slice() == b64 {
            return true;
        }
    }
    false
}

pub mod landlock {
    pub const EXECUTE: u64 = 1 << 0;
    pub const WRITE_FILE: u64 = 1 << 1;
    pub const READ_FILE: u64 = 1 << 2;
    pub const READ_DIR: u64 = 1 << 3;
    pub const REMOVE_DIR: u64 = 1 << 4;
    pub const REMOVE_FILE: u64 = 1 << 5;
    pub const MAKE_CHAR: u64 = 1 << 6;
    pub const MAKE_DIR: u64 = 1 << 7;
    pub const MAKE_REG: u64 = 1 << 8;
    pub const MAKE_SOCK: u64 = 1 << 9;
    pub const MAKE_FIFO: u64 = 1 << 10;
    pub const MAKE_BLOCK: u64 = 1 << 11;
    pub const MAKE_SYM: u64 = 1 << 12;
    pub const REFER: u64 = 1 << 13;
    pub const TRUNCATE: u64 = 1 << 14;

    pub struct Access {
        pub read: u64,
        pub write: u64,
        pub exec: u64,
    }

    impl Access {
        pub fn for_abi(abi: i32) -> Access {
            let mut write = WRITE_FILE
                | MAKE_REG
                | MAKE_DIR
                | REMOVE_FILE
                | REMOVE_DIR
                | MAKE_SYM
                | MAKE_FIFO
                | MAKE_SOCK
                | MAKE_CHAR
                | MAKE_BLOCK
                | REFER;
            if abi < 2 {
                write &= !REFER;
            }
            if abi >= 3 {
                write |= TRUNCATE;
            }
            Access {
                read: READ_FILE | READ_DIR,
                write,
                exec: EXECUTE,
            }
        }

        pub fn read_write(&self) -> u64 {
            self.read | self.write
        }

        pub fn all(&self) -> u64 {
            self.read | self.write | self.exec
        }
    }
}

pub const SYSTEM_EXEC_DIRS: [&str; 4] = ["/usr", "/usr/local", "/lib", "/lib64"];

pub const SYSTEM_READ_DIRS: [&str; 10] = [
    "/etc",
    "/var/lib/ca-certificates",
    "/var/cache/fontconfig",
    "/proc",
    "/sys",
    "/run",
    "/dev/shm",
    "/dev/dri",
    "/tmp/.X11-unix",
    "/tmp/.ICE-unix",
];

pub const HOME_READ_SUBDIRS: [&str; 4] = [".fonts", ".fontconfig", ".icons", ".themes"];

pub const DEV_DATA_DIRS: [&str; 4] = [
    "../data",
    "../../data",
    "../../../data",
    "../share/southstar",
];

pub const PREFIX_LIB_DIRS: [&str; 2] = ["../lib", "../lib64"];

pub const DEV_ROOT_MARKERS: [&str; 3] = [
    "../meson.build",
    "../../meson.build",
    "../../../meson.build",
];

pub const SECCOMP_ALLOWED: &[&str] = &[
    "accept",
    "accept4",
    "access",
    "arch_prctl",
    "bind",
    "brk",
    "capget",
    "chdir",
    "chmod",
    "clock_getres",
    "clock_getres_time64",
    "clock_gettime",
    "clock_gettime64",
    "clock_nanosleep",
    "clock_nanosleep_time64",
    "clone",
    "clone3",
    "close",
    "close_range",
    "connect",
    "copy_file_range",
    "creat",
    "dup",
    "dup2",
    "dup3",
    "epoll_create",
    "epoll_create1",
    "epoll_ctl",
    "epoll_pwait",
    "epoll_pwait2",
    "epoll_wait",
    "eventfd",
    "eventfd2",
    "exit",
    "exit_group",
    "faccessat",
    "faccessat2",
    "fadvise64",
    "fadvise64_64",
    "fallocate",
    "fchdir",
    "fchmod",
    "fchmodat",
    "fcntl",
    "fcntl64",
    "fdatasync",
    "flock",
    "fstat",
    "fstat64",
    "fstatat64",
    "fstatfs",
    "fstatfs64",
    "fsync",
    "ftruncate",
    "ftruncate64",
    "futex",
    "futex_time64",
    "futex_waitv",
    "futimesat",
    "getcpu",
    "getcwd",
    "getdents",
    "getdents64",
    "getegid",
    "getegid32",
    "geteuid",
    "geteuid32",
    "getgid",
    "getgid32",
    "getgroups",
    "getgroups32",
    "getitimer",
    "getpeername",
    "getpgid",
    "getpgrp",
    "getpid",
    "getppid",
    "getpriority",
    "getrandom",
    "getresgid",
    "getresgid32",
    "getresuid",
    "getresuid32",
    "getrlimit",
    "get_robust_list",
    "getrusage",
    "getsid",
    "getsockname",
    "getsockopt",
    "gettid",
    "gettimeofday",
    "getuid",
    "getuid32",
    "getxattr",
    "inotify_add_watch",
    "inotify_init",
    "inotify_init1",
    "inotify_rm_watch",
    "ioctl",
    "kill",
    "lgetxattr",
    "link",
    "linkat",
    "listen",
    "listxattr",
    "_llseek",
    "llistxattr",
    "lseek",
    "lstat",
    "lstat64",
    "madvise",
    "mbind",
    "membarrier",
    "memfd_create",
    "memfd_secret",
    "mincore",
    "mkdir",
    "mkdirat",
    "mlock",
    "mlock2",
    "mlockall",
    "mmap",
    "mmap2",
    "mprotect",
    "mremap",
    "msync",
    "munlock",
    "munlockall",
    "munmap",
    "nanosleep",
    "newfstatat",
    "open",
    "openat",
    "openat2",
    "pause",
    "pidfd_open",
    "pidfd_send_signal",
    "pipe",
    "pipe2",
    "pkey_alloc",
    "pkey_free",
    "pkey_mprotect",
    "poll",
    "ppoll",
    "ppoll_time64",
    "prctl",
    "pread64",
    "preadv",
    "preadv2",
    "prlimit64",
    "pselect6",
    "pselect6_time64",
    "pwrite64",
    "pwritev",
    "pwritev2",
    "read",
    "readahead",
    "readlink",
    "readlinkat",
    "readv",
    "recv",
    "recvfrom",
    "recvmmsg",
    "recvmmsg_time64",
    "recvmsg",
    "remap_file_pages",
    "rename",
    "renameat",
    "renameat2",
    "restart_syscall",
    "rmdir",
    "rseq",
    "rt_sigaction",
    "rt_sigpending",
    "rt_sigprocmask",
    "rt_sigqueueinfo",
    "rt_sigreturn",
    "rt_sigsuspend",
    "rt_sigtimedwait",
    "rt_sigtimedwait_time64",
    "rt_tgsigqueueinfo",
    "sched_getaffinity",
    "sched_getattr",
    "sched_getparam",
    "sched_get_priority_max",
    "sched_get_priority_min",
    "sched_getscheduler",
    "sched_rr_get_interval",
    "sched_setaffinity",
    "sched_setattr",
    "sched_setparam",
    "sched_setscheduler",
    "sched_yield",
    "select",
    "semctl",
    "semget",
    "semop",
    "semtimedop",
    "send",
    "sendfile",
    "sendfile64",
    "sendmmsg",
    "sendmsg",
    "sendto",
    "setitimer",
    "setpgid",
    "setpriority",
    "setrlimit",
    "set_robust_list",
    "setsid",
    "setsockopt",
    "set_tid_address",
    "shmat",
    "shmctl",
    "shmdt",
    "shmget",
    "shutdown",
    "sigaltstack",
    "signalfd",
    "signalfd4",
    "socket",
    "socketpair",
    "splice",
    "stat",
    "stat64",
    "statfs",
    "statfs64",
    "statx",
    "symlink",
    "symlinkat",
    "sync",
    "sync_file_range",
    "syncfs",
    "sysinfo",
    "tee",
    "tgkill",
    "time",
    "timer_create",
    "timer_delete",
    "timer_getoverrun",
    "timer_gettime",
    "timer_gettime64",
    "timer_settime",
    "timer_settime64",
    "timerfd_create",
    "timerfd_gettime",
    "timerfd_gettime64",
    "timerfd_settime",
    "timerfd_settime64",
    "times",
    "tkill",
    "truncate",
    "truncate64",
    "umask",
    "uname",
    "unlink",
    "unlinkat",
    "utime",
    "utimensat",
    "utimensat_time64",
    "utimes",
    "wait4",
    "waitid",
    "waitpid",
    "write",
    "writev",
];

pub fn seatbelt_subpath(profile: &mut Vec<u8>, dir: &[u8]) {
    profile.extend_from_slice(b"  (subpath \"");
    for &byte in dir {
        if byte == b'"' || byte == b'\\' {
            profile.push(b'\\');
        }
        profile.push(byte);
    }
    profile.extend_from_slice(b"\")\n");
}

pub const SEATBELT_HEAD: &[u8] = b"(version 1)\n\
(allow default)\n\
(deny file-write*)\n\
(allow file-write*\n\
\x20 (subpath \"/private/var/folders\")\n\
\x20 (subpath \"/private/tmp\")\n\
\x20 (subpath \"/tmp\")\n\
\x20 (subpath \"/dev\")\n";

pub fn zone_identifier(url: Option<&[u8]>) -> Vec<u8> {
    let mut zone = b"[ZoneTransfer]\r\nZoneId=3\r\n".to_vec();
    if let Some(url) = url.filter(|url| !url.is_empty()) {
        for key in [&b"HostUrl="[..], b"ReferrerUrl="] {
            zone.extend_from_slice(key);
            zone.extend_from_slice(url);
            zone.extend_from_slice(b"\r\n");
        }
    }
    zone
}

pub const ELEVATED_MESSAGE: &str = "Southstar is running as administrator.\n\
\n\
Southstar tried to restart itself without administrator rights but could not. You do not need administrator rights to browse the web, and it is much safer without them: if a web page exploited a flaw in the browser while it has administrator rights, it could take over your whole PC instead of being limited to your own user account.\n\
\n\
How to start Southstar normally:\n\
\x20 -  Close this window, then open Southstar with a normal click on its Start-menu or desktop icon. Do not choose \"Run as administrator\".\n\
\x20 -  If you started it from a Command Prompt or PowerShell, use an ordinary window that is not marked \"Administrator\".\n\
\x20 -  If a shortcut keeps starting it elevated, right-click the shortcut, choose Properties, then Advanced, and turn off \"Run as administrator\".\n\
\n\
Do you want to run as administrator anyway?\n\
\x20 -  No (recommended): quit, then reopen Southstar normally.\n\
\x20 -  Yes: run as administrator this once, at your own risk.";

pub const ROOT_MESSAGE: &str = "southstar: refusing to run as root.\n\
\x20 Web browsers process untrusted content; running as root exposes\n\
\x20 the whole system if the renderer is compromised.\n\
\x20 Re-run as an unprivileged user, or set NS_ALLOW_ROOT=1 to override.\n";
