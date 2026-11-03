use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const HW_BRIDGE_BIN: &str = "/usr/local/bin/qemu-hw-bridge";
const QUASAR_LAUNCHER2_PATH: &str = "/system/vendor/quasar/quasar_launcher2";
const QUASAR_LAUNCH_JSON_PATH: &str = "/system/vendor/quasar/launch.json";
const STUB_DIR: &str = "/tmp/.kuyase-stubs";
const LOOP_CAPTURE_FIFO: &str = "/tmp/loop_capture_pcm";

const CANONICAL_MAIND_LOG: &str = "/tmp/daemons_logs/maind.log";
const DATA_MAIND_LOG: &str = "/data/quasar/daemons_logs/maind.log";
const BKUPGRADE_MAIND_LOG: &str = "/bkupgrade/data/quasar/daemons_logs/maind.log";

#[cfg(target_os = "linux")]
extern "C" {
    fn signal(signum: i32, handler: usize) -> usize;
    fn sigprocmask(how: i32, set: *const u64, oldset: *mut u64) -> i32;
    fn setsid() -> i32;
    fn dup2(oldfd: i32, newfd: i32) -> i32;
    fn close(fd: i32) -> i32;
    fn unlink(pathname: *const u8) -> i32;
    fn _exit(status: i32) -> !;
}

/// Signal handler for `quasar_launcher2` stub in `Stub Mode`.
///
/// The stock `/system/vendor/quasar/q` script stops Quasar via:
/// ```sh
/// killall quasar_launcher2
/// sleep 5
/// killall -9 maind
/// killall -9 begemot 2> /dev/null
/// ```
/// When `killall quasar_launcher2` delivers `SIGTERM` (15), `quasar_launcher2` must remove
/// `/tmp/loop_capture_pcm` (so `yandexmini`'s `run.sh` can `mkfifo /tmp/loop_capture_pcm` on
/// subsequent `q start`/`q restart` without `File exists` on stderr) and exit immediately with
/// status `0`, while leaving the `maind` and `begemot` idle stubs alive to be reaped by
/// `killall -9 maind` and `killall -9 begemot` 5 seconds later.
#[cfg(target_os = "linux")]
extern "C" fn on_launcher_sigterm(_sig: i32) {
    unsafe {
        let _ = unlink(b"/tmp/loop_capture_pcm\0".as_ptr());
        _exit(0);
    }
}

#[cfg(target_os = "linux")]
fn setup_stub_signals(is_launcher2: bool) {
    const SIG_SETMASK: i32 = 2;
    const SIG_DFL: usize = 0;
    const SIGINT: i32 = 2;
    const SIGTERM: i32 = 15;
    unsafe {
        let empty_mask = [0u64; 16];
        sigprocmask(SIG_SETMASK, empty_mask.as_ptr(), std::ptr::null_mut());
        for sig in [1, 2, 3, 13, 15, 17, 18, 20, 21, 22] {
            signal(sig, SIG_DFL);
        }
        if is_launcher2 {
            signal(SIGTERM, on_launcher_sigterm as *const () as usize);
            signal(SIGINT, on_launcher_sigterm as *const () as usize);
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn setup_stub_signals(_is_launcher2: bool) {}

/// Detaches standard streams (`stdin`, `stdout`, `stderr`) to `/dev/null` and closes inherited
/// file descriptors so shell command substitutions such as `$(/system/vendor/quasar/q start)`
/// or `$(/system/vendor/quasar/q restart)` never block waiting for EOF on an inherited pipe.
#[cfg(target_os = "linux")]
fn detach_stdio_and_extra_fds() {
    use std::os::unix::io::AsRawFd;
    unsafe {
        let _ = setsid();
    }
    if let Ok(devnull) = OpenOptions::new().read(true).write(true).open("/dev/null") {
        let null_fd = devnull.as_raw_fd();
        unsafe {
            let _ = dup2(null_fd, 0);
            let _ = dup2(null_fd, 1);
            let _ = dup2(null_fd, 2);
        }
    }
    unsafe {
        for fd in 3..64 {
            let _ = close(fd);
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn detach_stdio_and_extra_fds() {}

/// Sets the kernel task comm (`/proc/self/comm`, up to 15 bytes) so BusyBox `pidof` and `killall`
/// match the stub process by name (`quasar_launcher`, `maind`, `begemot`) without matching the
/// primary `qemu-hw-bridge` daemon.
fn set_process_comm(comm: &str) {
    let truncated = &comm[..comm.len().min(15)];
    let _ = fs::write("/proc/self/comm", format!("{}\n", truncated));
}

/// Checks if a process with the given command name or `argv[0]` basename is currently alive in `/proc`.
pub fn is_process_running(target_name: &str) -> bool {
    let target_comm = &target_name[..target_name.len().min(15)];
    let self_pid = std::process::id();
    let Ok(entries) = fs::read_dir("/proc") else {
        return false;
    };
    for entry in entries.flatten() {
        let fname = entry.file_name();
        let Some(pid_str) = fname.to_str() else {
            continue;
        };
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };
        if pid == self_pid {
            continue;
        }
        // Ignore zombie processes (`State:\tZ`) so `quasar_status` / `pidof` checks see live tasks only.
        if let Ok(status) = fs::read_to_string(format!("/proc/{}/status", pid)) {
            if status.lines().any(|l| l.starts_with("State:") && l.contains('Z')) {
                continue;
            }
        }
        if let Ok(comm) = fs::read_to_string(format!("/proc/{}/comm", pid)) {
            if comm.trim() == target_comm || comm.trim() == target_name {
                return true;
            }
        }
        if let Ok(cmdline) = fs::read(format!("/proc/{}/cmdline", pid)) {
            if let Some(arg0) = cmdline.split(|&b| b == 0).next() {
                if let Ok(arg0_str) = std::str::from_utf8(arg0) {
                    let base = Path::new(arg0_str)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(arg0_str);
                    if base == target_name {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Checks whether `target` (or its canonical path) is already a mount point in `/proc/mounts`.
pub fn is_path_mounted(target: &str) -> bool {
    let Ok(mounts) = fs::read_to_string("/proc/mounts") else {
        return false;
    };
    let canonical = fs::canonicalize(target)
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()));
    for line in mounts.lines() {
        let mut parts = line.split_whitespace();
        let _src = parts.next();
        let Some(mount_point) = parts.next() else {
            continue;
        };
        if mount_point == target {
            return true;
        }
        if let Some(ref canon) = canonical {
            if mount_point == canon {
                return true;
            }
        }
    }
    false
}

/// Bind-mounts `src` over `dst` in the Linux VFS mount table if `src` and `dst` exist and `dst`
/// is not already bind-mounted.
pub fn bind_mount_file(src: &str, dst: &str) -> bool {
    if !Path::new(src).exists() || !Path::new(dst).exists() {
        return false;
    }
    if is_path_mounted(dst) {
        return true;
    }
    Command::new("mount")
        .args(["--bind", src, dst])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Prepares `/tmp/.kuyase-stubs/maind` and `/tmp/.kuyase-stubs/begemot` pointing to
/// `/usr/local/bin/qemu-hw-bridge` and spawns both with `--kuyase-idle-stub` if not already running.
///
/// Why both `maind` and `begemot` idle stubs are required:
/// In `/system/vendor/quasar/q`, `quasar_stop()` executes:
/// ```sh
/// killall quasar_launcher2
/// sleep 5
/// killall -9 maind
/// killall -9 begemot 2> /dev/null
/// ```
/// In POSIX `/bin/sh`, `quasar_stop()` returns the exit status of its final command
/// (`killall -9 begemot 2> /dev/null`), while `killall -9 maind` has no stderr redirect.
/// Having lightweight idle stubs for both `maind` and `begemot` ensures `killall -9 maind` emits
/// zero stderr and `killall -9 begemot` returns exit code `0`.
fn ensure_idle_stubs_running() {
    let _ = fs::create_dir_all(STUB_DIR);
    for name in ["maind", "begemot"] {
        let stub_path = format!("{}/{}", STUB_DIR, name);
        ensure_stub_binary_at(&stub_path);
        if !is_process_running(name) {
            let _ = Command::new(&stub_path)
                .arg("--kuyase-idle-stub")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }
    }
}

fn ensure_stub_binary_at(stub_path: &str) {
    let p = Path::new(stub_path);
    if !p.exists() && fs::symlink_metadata(p).is_err() {
        // Prefer creating an empty file and bind-mounting `qemu-hw-bridge` over it so the kernel
        // dentry name (`/proc/<pid>/comm` and `/proc/<pid>/exe`) matches the stub basename;
        // fall back to a symlink if bind-mounting is unavailable.
        let _ = fs::write(p, b"");
        if !bind_mount_file(HW_BRIDGE_BIN, stub_path) {
            let _ = fs::remove_file(p);
            #[cfg(unix)]
            {
                let _ = std::os::unix::fs::symlink(HW_BRIDGE_BIN, p);
            }
        }
    } else if p.is_file()
        && fs::metadata(p).map(|m| m.len() == 0).unwrap_or(false)
        && !bind_mount_file(HW_BRIDGE_BIN, stub_path)
    {
        let _ = fs::remove_file(p);
        #[cfg(unix)]
        {
            let _ = std::os::unix::fs::symlink(HW_BRIDGE_BIN, p);
        }
    }
}

fn run_idle_daemon_stub(applet_name: &str) -> ! {
    set_process_comm(applet_name);
    setup_stub_signals(false);
    detach_stdio_and_extra_fds();
    loop {
        thread::sleep(Duration::from_secs(3600));
    }
}

fn run_quasar_launcher2_stub() -> ! {
    set_process_comm("quasar_launcher");
    setup_stub_signals(true);
    // Remove `/tmp/loop_capture_pcm` created by `yandexmini`'s `/system/vendor/quasar/run.sh`
    // (`mkfifo /tmp/loop_capture_pcm`) so subsequent `q start` / `q restart` invocations
    // never print `mkfifo: /tmp/loop_capture_pcm: File exists`.
    let _ = fs::remove_file(LOOP_CAPTURE_FIFO);
    ensure_idle_stubs_running();
    detach_stdio_and_extra_fds();
    loop {
        ensure_idle_stubs_running();
        thread::sleep(Duration::from_millis(250));
    }
}

/// Classification of `argv` for multi-call stub applet dispatch.
#[derive(Debug, PartialEq, Eq)]
pub enum StubAppletKind {
    QuasarLauncher2,
    IdleDaemon(String),
}

/// Classifies `argv` to determine whether `qemu-hw-bridge` was invoked as a `Stub Mode`
/// lifecycle applet (`quasar_launcher2`, `maind`, or `begemot`).
pub fn classify_stub_applet_args(args: &[String]) -> Option<StubAppletKind> {
    let arg0 = args.first().map(|s| s.as_str()).unwrap_or("");
    let arg0_base = Path::new(arg0)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(arg0);
    let arg1 = args.get(1).map(|s| s.as_str()).unwrap_or("");

    if arg0_base == "quasar_launcher2" || arg1 == "--quasar-launcher2-stub" {
        return Some(StubAppletKind::QuasarLauncher2);
    }
    if arg1 == "--kuyase-idle-stub" || arg0_base == "maind" || arg0_base == "begemot" {
        let daemon_name = if arg0_base == "maind" || arg0_base == "begemot" {
            arg0_base.to_string()
        } else {
            args.get(2)
                .cloned()
                .unwrap_or_else(|| "maind".to_string())
        };
        return Some(StubAppletKind::IdleDaemon(daemon_name));
    }
    None
}

/// Dispatches `quasar_launcher2` or `maind`/`begemot` idle stubs when `qemu-hw-bridge` is
/// invoked via bind-mount or `/tmp/.kuyase-stubs/*`. Returns `true` if handled.
pub fn maybe_run_stub_applet() -> bool {
    let args: Vec<String> = env::args().collect();
    match classify_stub_applet_args(&args) {
        Some(StubAppletKind::QuasarLauncher2) => {
            run_quasar_launcher2_stub();
        }
        Some(StubAppletKind::IdleDaemon(name)) => {
            run_idle_daemon_stub(&name);
        }
        None => false,
    }
}

/// Unifies `/data/quasar/daemons_logs/maind.log`, `/bkupgrade/data/quasar/daemons_logs/maind.log`,
/// and `/tmp/daemons_logs/maind.log` via symlink.
///
/// Firmware `quasar.cfg` files define `"LOGS": "${DATA}/daemons_logs"` (`/data/quasar/daemons_logs/maind.log`),
/// while `Hardware Bridge` and companion daemons read/write `/tmp/daemons_logs/maind.log`.
/// Symlinking both paths to `/tmp/daemons_logs/maind.log` ensures all tools observe a single log stream.
pub fn ensure_unified_log_symlinks_under(root: &Path) {
    let canonical_log = root.join(CANONICAL_MAIND_LOG.trim_start_matches('/'));
    if let Some(parent) = canonical_log.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if !canonical_log.exists() {
        let _ = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&canonical_log);
    }

    for dir_rel in [
        "data/quasar/daemons_logs",
        "data/quasar/data",
        "data/quasar/logs",
        "bkupgrade/data/quasar/daemons_logs",
        "bkupgrade/data/quasar/data",
        "bkupgrade/data/quasar/logs",
    ] {
        let _ = fs::create_dir_all(root.join(dir_rel));
    }

    let target_for_link = if root == Path::new("/") {
        PathBuf::from(CANONICAL_MAIND_LOG)
    } else {
        canonical_log.clone()
    };

    for link_rel in [
        DATA_MAIND_LOG.trim_start_matches('/'),
        BKUPGRADE_MAIND_LOG.trim_start_matches('/'),
    ] {
        let link_path = root.join(link_rel);
        if let Some(parent) = link_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        // If `data` is already a symlink to `bkupgrade/data`, both paths may resolve to the same entry.
        let is_already_valid_symlink = fs::symlink_metadata(&link_path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
            && fs::read_link(&link_path)
                .map(|t| t == target_for_link || t == Path::new(CANONICAL_MAIND_LOG))
                .unwrap_or(false);
        if is_already_valid_symlink {
            continue;
        }
        if let Ok(meta) = fs::symlink_metadata(&link_path) {
            if !meta.file_type().is_symlink() && meta.is_file() {
                if let Ok(existing_bytes) = fs::read(&link_path) {
                    if !existing_bytes.is_empty() {
                        if let Ok(mut f) = OpenOptions::new().append(true).open(&canonical_log) {
                            let _ = f.write_all(&existing_bytes);
                        }
                    }
                }
            }
            let _ = fs::remove_file(&link_path);
        }
        #[cfg(unix)]
        {
            let _ = std::os::unix::fs::symlink(&target_for_link, &link_path);
        }
        #[cfg(not(unix))]
        {
            let _ = fs::write(&link_path, b"");
        }
    }
}

/// Populates `/sys/bus/sdio/devices/mmc1:0001:1/{vendor,device}` and default
/// `/sys/devices/platform/pcba_ids/pcba_ids_drv/pcba_ids`.
///
/// Why this exists:
/// 1. On `0.13+` / `0.369+` / `0.373+` firmwares, `maind` executes `/usr/bin/yandex-wifichipmodel`
///    during initialization (even for `q version`, `q device-id`, `q getprop`, and `led_util`),
///    which runs `cat /sys/bus/sdio/devices/*/vendor` and `cat /sys/bus/sdio/devices/*/device`.
///    Providing Broadcom BCM43455 SDIO IDs (`0x02d0` / `0xa9bf`) silences `cat: can't open ...` on stderr.
/// 2. `pcba_ids.cpp` inside `maind` reads `/sys/devices/platform/pcba_ids/pcba_ids_drv/pcba_ids`
///    and logs `[warning] [pcba_ids.cpp:15] PcbaIds node doesn't exist` when the node is absent.
pub fn ensure_platform_sysfs_nodes_under(root: &Path) {
    let sdio_dir = root.join("sys/bus/sdio/devices/mmc1:0001:1");
    let target_sdio_dir = if fs::create_dir_all(&sdio_dir).is_ok() {
        sdio_dir
    } else {
        // Fallback for Windows NTFS host unit tests where `:` is reserved in directory names.
        let fallback = root.join("sys/bus/sdio/devices/mmc1_0001_1");
        let _ = fs::create_dir_all(&fallback);
        fallback
    };
    let _ = fs::write(target_sdio_dir.join("vendor"), "0x02d0\n");
    let _ = fs::write(target_sdio_dir.join("device"), "0xa9bf\n");

    let pcba_dir = root.join("sys/devices/platform/pcba_ids/pcba_ids_drv");
    let _ = fs::create_dir_all(&pcba_dir);
    let pcba_file = pcba_dir.join("pcba_ids");
    let needs_default = fs::metadata(&pcba_file)
        .map(|m| m.len() == 0)
        .unwrap_or(true);
    if needs_default {
        let _ = fs::write(&pcba_file, "[{\"name\":\"main\",\"assembly\":\"1\"}]\n");
    }
}

/// Populates Linkplay `fnvram` status and JSON files for `yandexmini` (`1.266.4.40`).
///
/// Why this exists:
/// Disassembly of `/system/vendor/quasar/libs/libnvram.so` (`verify_fnvram_statu` at `0x1330` and
/// `fnvram_bufget` at `0x1440`) showed that on `yandexmini`:
/// 1. `libnvram.so` accesses `/vendor/new_nvram_config_status` and `/vendor/new_nvram_factory_status`
///    (where `/vendor` is a symlink to `/bkupgrade` created on physical hardware by `S05linkplay.sh`).
/// 2. `verify_fnvram_statu` parses the status file with `strtol(buf, 0, 10)` and requires integer `3`
///    (`0` prints `"fnvram status file not found!"`, `1` and `2` trigger backup restore warnings).
/// 3. `fnvram_bufget` parses `/vendor/new_nvram_config` and `/vendor/new_nvram_factory` with `json-c`
///    and looks up `"mv_uuid"`, allowing `q device-id` and `q full-info` to output `Device ID: EMU0001`.
pub fn ensure_linkplay_fnvram_under(root: &Path) {
    let bkupgrade_dir = root.join("bkupgrade");
    let _ = fs::create_dir_all(&bkupgrade_dir);

    let vendor_link = root.join("vendor");
    if !vendor_link.exists() && fs::symlink_metadata(&vendor_link).is_err() {
        #[cfg(unix)]
        {
            let target = if root == Path::new("/") {
                PathBuf::from("/bkupgrade")
            } else {
                bkupgrade_dir.clone()
            };
            let _ = std::os::unix::fs::symlink(target, &vendor_link);
        }
    }

    for status_name in ["new_nvram_config_status", "new_nvram_factory_status"] {
        let p = bkupgrade_dir.join(status_name);
        if !p.exists() {
            let _ = fs::write(p, "3\n");
        }
    }

    const FNVRAM_JSON: &str = "{\"mv_uuid\":\"EMU0001\"}\n";
    for cfg_name in [
        "new_nvram_config",
        "new_nvram_config_bak",
        "new_nvram_factory",
        "new_nvram_factory_bak",
    ] {
        let p = bkupgrade_dir.join(cfg_name);
        if !p.exists() {
            let _ = fs::write(p, FNVRAM_JSON);
        }
    }
}

/// Ensures fallback `/system/vendor/quasar/set_volume.sh` exists on boards (such as `cucumber`)
/// where hardware volume is normally handled by `hardvold` and `set_volume.sh` is omitted from the image.
pub fn ensure_fallback_set_volume_sh_under(root: &Path) {
    let quasar_dir = root.join("system/vendor/quasar");
    if !quasar_dir.is_dir() {
        return;
    }
    let script_path = quasar_dir.join("set_volume.sh");
    if !script_path.exists() {
        let script = "#!/bin/sh\namixer set 'Master' \"$1%\" >/dev/null 2>&1 || true\n";
        if fs::write(&script_path, script).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755));
            }
        }
    }
}

/// Ensures a valid `wlan0` network interface exists in the guest kernel netlink table.
///
/// Why this exists:
/// `q wifi-info` (`maind --wifi-info`) instantiates `RtnlLinkMonitor`, which queries the kernel
/// via netlink `RTM_GETLINK` and aborts with `std::runtime_error: network device wlan0 does not exists`
/// if only `/sys/class/net/wlan0/address` exists without a kernel netdev.
/// Because `CONFIG_DUMMY` is not enabled in the guest kernel whereas `sit0` (`CONFIG_IPV6_SIT=y`)
/// is always present at boot, we try `ip link add wlan0 type dummy` and fall back to renaming
/// `sit0` to `wlan0` when no `--lan-tap` (`52:4b:59:*`) interface is attached.
pub fn ensure_virtual_wlan0_netdev() {
    let wlan0_exists = Command::new("ip")
        .args(["link", "show", "wlan0"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !wlan0_exists {
        // Check if a `--lan-tap` virtio NIC (`52:4b:59:*`) is present and rename it to `wlan0` first.
        let mut renamed_lan_tap = false;
        if let Ok(out) = Command::new("ip")
            .args(["-o", "link", "show"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if line.contains("52:4b:59:") {
                    if let Some(ifname) = line
                        .split(':')
                        .nth(1)
                        .map(|s| s.trim().split('@').next().unwrap_or("").trim())
                    {
                        if !ifname.is_empty() && ifname != "wlan0" {
                            let _ = Command::new("ip")
                                .args(["link", "set", ifname, "down"])
                                .stdin(Stdio::null())
                                .stdout(Stdio::null())
                                .stderr(Stdio::null())
                                .status();
                            if Command::new("ip")
                                .args(["link", "set", ifname, "name", "wlan0"])
                                .stdin(Stdio::null())
                                .stdout(Stdio::null())
                                .stderr(Stdio::null())
                                .status()
                                .map(|s| s.success())
                                .unwrap_or(false)
                            {
                                renamed_lan_tap = true;
                                break;
                            }
                        }
                    }
                }
            }
        }

        if !renamed_lan_tap {
            let dummy_ok = Command::new("ip")
                .args(["link", "add", "wlan0", "type", "dummy"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !dummy_ok {
                let _ = Command::new("ip")
                    .args(["link", "set", "sit0", "name", "wlan0"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            let _ = Command::new("ip")
                .args(["link", "set", "wlan0", "address", "52:54:00:12:34:57"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }

    let up_ok = Command::new("ip")
        .args(["link", "set", "wlan0", "up"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !up_ok {
        let _ = Command::new("ifconfig")
            .args(["wlan0", "up"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    let _ = fs::create_dir_all("/sys/class/net/wlan0");
    let _ = fs::write("/sys/class/net/wlan0/address", "52:54:00:12:34:57\n");
}

/// Bind-mounts `/usr/local/bin/qemu-hw-bridge` over `/system/vendor/quasar/quasar_launcher2`
/// and starts the `quasar_launcher2` supervisor stub in `Stub Mode`.
fn ensure_quasar_launcher2_stub_running() {
    let _ = fs::remove_file(LOOP_CAPTURE_FIFO);
    if Path::new(QUASAR_LAUNCHER2_PATH).exists() && Path::new(HW_BRIDGE_BIN).exists() {
        bind_mount_file(HW_BRIDGE_BIN, QUASAR_LAUNCHER2_PATH);
    }
    ensure_idle_stubs_running();
    if !is_process_running("quasar_launcher2") {
        let launch_bin = if Path::new(QUASAR_LAUNCHER2_PATH).exists() {
            QUASAR_LAUNCHER2_PATH
        } else {
            HW_BRIDGE_BIN
        };
        let mut cmd = Command::new(launch_bin);
        if launch_bin == HW_BRIDGE_BIN {
            cmd.arg("--quasar-launcher2-stub");
        }
        cmd.arg(QUASAR_LAUNCH_JSON_PATH)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let _ = cmd.spawn();
    }

    // Wait briefly so `pidof quasar_launcher2`, `pidof maind`, and `pidof begemot` are visible
    // in `/proc` before the Hardware Bridge opens `:8091` for host `--exec` commands.
    let deadline = Instant::now() + Duration::from_millis(200);
    while Instant::now() < deadline {
        if is_process_running("quasar_launcher2")
            && is_process_running("maind")
            && is_process_running("begemot")
        {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// Prepares all guest runtime environment stubs on `qemu-hw-bridge` startup so existing
/// `work/*-system.ext4` images support unmodified `/system/vendor/quasar/q` commands without
/// rebuilding `.ext4` images.
pub fn prepare_runtime_environment(_board_slug: &str, full_mode: bool) {
    let root = Path::new("/");
    ensure_unified_log_symlinks_under(root);
    ensure_platform_sysfs_nodes_under(root);
    ensure_linkplay_fnvram_under(root);
    ensure_fallback_set_volume_sh_under(root);
    ensure_virtual_wlan0_netdev();
    if !full_mode {
        ensure_quasar_launcher2_stub_running();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_root(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "kuyase-runtime-env-test-{}-{}-{}",
            tag,
            std::process::id(),
            nanos
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn test_classify_stub_applet_args() {
        assert_eq!(
            classify_stub_applet_args(&[
                "/system/vendor/quasar/quasar_launcher2".to_string(),
                "/system/vendor/quasar/launch.json".to_string()
            ]),
            Some(StubAppletKind::QuasarLauncher2)
        );
        assert_eq!(
            classify_stub_applet_args(&[
                "/usr/local/bin/qemu-hw-bridge".to_string(),
                "--quasar-launcher2-stub".to_string()
            ]),
            Some(StubAppletKind::QuasarLauncher2)
        );
        assert_eq!(
            classify_stub_applet_args(&[
                "/tmp/.kuyase-stubs/maind".to_string(),
                "--kuyase-idle-stub".to_string()
            ]),
            Some(StubAppletKind::IdleDaemon("maind".to_string()))
        );
        assert_eq!(
            classify_stub_applet_args(&[
                "/tmp/.kuyase-stubs/begemot".to_string(),
                "--kuyase-idle-stub".to_string()
            ]),
            Some(StubAppletKind::IdleDaemon("begemot".to_string()))
        );
        assert_eq!(
            classify_stub_applet_args(&["/usr/local/bin/qemu-hw-bridge".to_string()]),
            None
        );
    }

    #[test]
    fn test_platform_sysfs_and_linkplay_fnvram_generation() {
        let root = make_temp_root("sysfs-fnvram");
        ensure_platform_sysfs_nodes_under(&root);
        ensure_linkplay_fnvram_under(&root);

        let sdio_dir = if root.join("sys/bus/sdio/devices/mmc1:0001:1").exists() {
            root.join("sys/bus/sdio/devices/mmc1:0001:1")
        } else {
            root.join("sys/bus/sdio/devices/mmc1_0001_1")
        };
        assert_eq!(
            fs::read_to_string(sdio_dir.join("vendor")).unwrap(),
            "0x02d0\n"
        );
        assert_eq!(
            fs::read_to_string(sdio_dir.join("device")).unwrap(),
            "0xa9bf\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("sys/devices/platform/pcba_ids/pcba_ids_drv/pcba_ids"))
                .unwrap(),
            "[{\"name\":\"main\",\"assembly\":\"1\"}]\n"
        );

        // Existing board-specific pcba_ids must not be overwritten.
        fs::write(
            root.join("sys/devices/platform/pcba_ids/pcba_ids_drv/pcba_ids"),
            "[{\"name\":\"main\",\"assembly\":\"custom\"}]\n",
        )
        .unwrap();
        ensure_platform_sysfs_nodes_under(&root);
        assert_eq!(
            fs::read_to_string(root.join("sys/devices/platform/pcba_ids/pcba_ids_drv/pcba_ids"))
                .unwrap(),
            "[{\"name\":\"main\",\"assembly\":\"custom\"}]\n"
        );

        assert_eq!(
            fs::read_to_string(root.join("bkupgrade/new_nvram_config_status")).unwrap(),
            "3\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("bkupgrade/new_nvram_factory_status")).unwrap(),
            "3\n"
        );
        for cfg in [
            "new_nvram_config",
            "new_nvram_config_bak",
            "new_nvram_factory",
            "new_nvram_factory_bak",
        ] {
            assert_eq!(
                fs::read_to_string(root.join("bkupgrade").join(cfg)).unwrap(),
                "{\"mv_uuid\":\"EMU0001\"}\n"
            );
        }

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_unified_logs_and_fallback_set_volume() {
        let root = make_temp_root("logs-vol");
        fs::create_dir_all(root.join("system/vendor/quasar")).unwrap();
        ensure_unified_log_symlinks_under(&root);
        ensure_fallback_set_volume_sh_under(&root);

        assert!(root.join("tmp/daemons_logs/maind.log").exists());
        assert!(root.join("data/quasar/daemons_logs/maind.log").exists());
        assert!(root.join("data/quasar/data").is_dir());
        assert!(root.join("system/vendor/quasar/set_volume.sh").exists());

        let _ = fs::remove_dir_all(&root);
    }
}
