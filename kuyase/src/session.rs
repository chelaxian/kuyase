use protocol::normalize_board_slug;
use std::collections::VecDeque;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

pub const EMBEDDED_HW_BRIDGE: &[u8] = include_bytes!("../../guest-init/bin/qemu-hw-bridge");

pub fn bridge_connect_timeout() -> Duration {
    let secs = env::var("KUYASE_BOOT_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(180);
    Duration::from_secs(secs)
}

pub fn resolve_repo_root() -> PathBuf {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for ancestor in cwd.ancestors().take(4) {
        if ancestor.join("guest-init").is_dir()
            || ancestor.join("work").is_dir()
            || ancestor.join("Releases").is_dir()
        {
            return ancestor.to_path_buf();
        }
    }
    if let Ok(exe) = env::current_exe() {
        let has_local_kernel = |dir: &Path| {
            dir.join("kuyase-kernel-arm64.img").is_file() || dir.join("Image").is_file()
        };
        if let Some(exe_dir) = exe.parent() {
            if has_local_kernel(exe_dir) && !exe_dir.join("..").join("Cargo.toml").is_file() {
                return exe_dir.to_path_buf();
            }
        }
        for ancestor in exe.ancestors().skip(1).take(4) {
            if ancestor.join("guest-init").is_dir()
                || ancestor.join("work").is_dir()
                || ancestor.join("Releases").is_dir()
            {
                return ancestor.to_path_buf();
            }
        }
        if let Some(exe_dir) = exe.parent() {
            if has_local_kernel(exe_dir) {
                return exe_dir.to_path_buf();
            }
        }
    }
    cwd
}

fn find_in_path(bin_names: &[&str]) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        for name in bin_names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn resolve_qemu_binary(root: &Path) -> Result<PathBuf, String> {
    let local_candidates = [
        root.join("qemu-system-aarch64.exe"),
        root.join("qemu-system-aarch64"),
        root.join("bin").join("qemu-system-aarch64.exe"),
        root.join("bin").join("qemu-system-aarch64"),
        root.join("Releases").join("qemu-system-aarch64.exe"),
        root.join("Releases").join("qemu-system-aarch64"),
    ];
    for c in &local_candidates {
        if c.is_file() {
            return Ok(c.clone());
        }
    }

    if let Some(p) = find_in_path(&["qemu-system-aarch64.exe", "qemu-system-aarch64"]) {
        return Ok(p);
    }

    let system_candidates = [
        PathBuf::from(r"C:\Program Files\qemu\qemu-system-aarch64.exe"),
        PathBuf::from("/usr/bin/qemu-system-aarch64"),
        PathBuf::from("/usr/local/bin/qemu-system-aarch64"),
    ];
    for c in &system_candidates {
        if c.is_file() {
            return Ok(c.clone());
        }
    }

    Err(
        "qemu-system-aarch64 not found in ./bin, PATH, or C:\\Program Files\\qemu\\qemu-system-aarch64.exe"
            .to_string(),
    )
}

pub fn resolve_kernel_image(root: &Path) -> Result<PathBuf, String> {
    let candidates = [
        root.join("kuyase-kernel-arm64.img"),
        root.join("kernel").join("kuyase-kernel-arm64.img"),
        root.join("Releases").join("kuyase-kernel-arm64.img"),
        root.join("bin").join("kuyase-kernel-arm64.img"),
        root.join("Image"),
        root.join("kernel").join("Image"),
        root.join("Releases").join("Image"),
        root.join("bin").join("Image"),
    ];
    for c in &candidates {
        if c.is_file() {
            return Ok(c.clone());
        }
    }
    Err(format!(
        "Guest Linux kernel not found in {}/kuyase-kernel-arm64.img, {}/kernel/kuyase-kernel-arm64.img, {}/Releases/kuyase-kernel-arm64.img, or {}/kernel/Image",
        root.display(),
        root.display(),
        root.display(),
        root.display()
    ))
}

fn is_valid_elf(path: &Path) -> Option<SystemTime> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() < 64 {
        return None;
    }
    let mut f = File::open(path).ok()?;
    let mut magic = [0u8; 4];
    use std::io::Read;
    f.read_exact(&mut magic).ok()?;
    if &magic == b"\x7fELF" {
        Some(meta.modified().unwrap_or(SystemTime::UNIX_EPOCH))
    } else {
        None
    }
}

pub fn resolve_bridge_binary(root: &Path) -> Result<PathBuf, String> {
    let candidates = [
        root.join("target")
            .join("armv7-unknown-linux-musleabihf")
            .join("release")
            .join("qemu-hw-bridge"),
        root.join("hw-bridge")
            .join("target")
            .join("armv7-unknown-linux-musleabihf")
            .join("release")
            .join("qemu-hw-bridge"),
        root.join("guest-init").join("bin").join("qemu-hw-bridge"),
        root.join("Releases").join("qemu-hw-bridge"),
        root.join("qemu-hw-bridge"),
    ];

    let mut newest: Option<(PathBuf, SystemTime)> = None;
    for c in &candidates {
        if let Some(mtime) = is_valid_elf(c) {
            if newest.as_ref().map(|(_, t)| mtime > *t).unwrap_or(true) {
                newest = Some((c.clone(), mtime));
            }
        }
    }

    if let Some((best_path, _)) = newest {
        let init_bin = root.join("guest-init").join("bin").join("qemu-hw-bridge");
        if best_path != init_bin && root.join("guest-init").is_dir() {
            if let Some(parent) = init_bin.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::copy(&best_path, &init_bin);
        }
        return Ok(best_path);
    }

    if EMBEDDED_HW_BRIDGE.len() >= 64 && &EMBEDDED_HW_BRIDGE[0..4] == b"\x7fELF" {
        let work_dir = root.join("work");
        let _ = fs::create_dir_all(&work_dir);
        let extracted = work_dir.join("qemu-hw-bridge.bin");
        fs::write(&extracted, EMBEDDED_HW_BRIDGE)
            .map_err(|e| format!("failed to write embedded qemu-hw-bridge: {}", e))?;
        return Ok(extracted);
    }

    Err("qemu-hw-bridge binary not found on disk or in embedded resources".to_string())
}

pub fn discover_guest_images(root: &Path) -> Vec<(String, PathBuf)> {
    let work_dir = root.join("work");
    let Ok(entries) = fs::read_dir(&work_dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.ends_with("-system.ext4") && !name.ends_with("-system-orig.ext4") {
            let board = normalize_board_slug(name.trim_end_matches("-system.ext4"));
            found.push((board, path));
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

fn find_board_image_in_work_dir(work_dir: &Path, input: &str) -> Option<(String, PathBuf)> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
        return None;
    }
    let slug = normalize_board_slug(trimmed);
    let raw = trimmed.to_ascii_lowercase().replace(' ', "-");
    let raw_img = work_dir.join(format!("{}-system.ext4", raw));
    if raw_img.is_file() {
        return Some((slug, raw_img));
    }
    let img = work_dir.join(format!("{}-system.ext4", slug));
    if img.is_file() {
        return Some((slug, img));
    }
    None
}

pub fn resolve_board_and_image(
    root: &Path,
    requested_board: Option<&str>,
) -> Result<(String, PathBuf), String> {
    let work_dir = root.join("work");
    if let Some(req) = requested_board {
        let trimmed = req.trim();
        if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("auto") {
            if let Some(found) = find_board_image_in_work_dir(&work_dir, trimmed) {
                return Ok(found);
            }
            let slug = normalize_board_slug(trimmed);
            let img = work_dir.join(format!("{}-system.ext4", slug));
            return Err(format!(
                "Guest Image not found for board '{}': expected {}",
                slug,
                img.display()
            ));
        }
    }

    let last_board_file = work_dir.join(".last-board");
    if let Ok(last) = fs::read_to_string(&last_board_file) {
        if let Some(found) = find_board_image_in_work_dir(&work_dir, &last) {
            return Ok(found);
        }
    }

    let discovered = discover_guest_images(root);
    if let Some((slug, img)) = discovered.into_iter().next() {
        return Ok((slug, img));
    }

    Err(format!(
        "No *-system.ext4 Guest Images found in {}/work",
        root.display()
    ))
}

pub const DEFAULT_LAN_MAC_BASE: &str = "52:4b:59:00:00:01";

pub fn default_lan_mac(port_offset: u16) -> String {
    let last_octet = ((port_offset as u32) % 254) + 1;
    format!("52:4b:59:00:00:{:02x}", last_octet)
}

#[derive(Clone, Debug)]
pub struct SessionConfig {
    pub root_dir: PathBuf,
    pub board: String,
    pub board_cmdline: String,
    pub mode: String,
    pub snapshot: bool,
    pub persist_data: bool,
    pub bind_all: bool,
    pub lan_tap: Option<String>,
    pub lan_mac: Option<String>,
    pub port_offset: u16,
    pub bridge_port: u16,
    pub adb_port: u16,
    pub sdk_port: Option<u16>,
    pub drive_img: PathBuf,
}

impl SessionConfig {
    pub fn for_board(root: &Path, board: Option<&str>) -> Result<Self, String> {
        let (slug, drive_img) = resolve_board_and_image(root, board)?;
        let images_dir = drive_img.parent().unwrap_or(root);
        let data_img_exists =
            crate::image_builder::persistent_data_image_path(images_dir, &slug).is_file();
        let env_persist = env::var("KUYASE_PERSIST_DATA")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let env_bind_all = env::var("KUYASE_BIND_ALL")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let env_lan_tap = env::var("KUYASE_LAN_TAP")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let env_lan_mac = env::var("KUYASE_LAN_MAC")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        Ok(Self {
            root_dir: root.to_path_buf(),
            board: slug.clone(),
            board_cmdline: slug,
            mode: "stub".to_string(),
            snapshot: false,
            persist_data: data_img_exists || env_persist,
            bind_all: env_bind_all,
            lan_tap: env_lan_tap,
            lan_mac: env_lan_mac,
            port_offset: 0,
            bridge_port: 18091,
            adb_port: 15555,
            sdk_port: Some(18090),
            drive_img,
        })
    }

    pub fn apply_port_offset(&mut self, offset: u16) {
        self.port_offset = offset;
        if offset > 0 {
            self.bridge_port = self.bridge_port.saturating_add(offset);
            self.adb_port = self.adb_port.saturating_add(offset);
            self.sdk_port = self.sdk_port.map(|p| p.saturating_add(offset));
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SerialSnapshot {
    pub lines: Vec<String>,
    pub active_line: String,
    pub cursor_col: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum AnsiParseState {
    #[default]
    Ground,
    Escape,
    Csi(String),
    Osc,
}

#[derive(Debug, Default)]
pub struct SerialScreenBuffer {
    completed_lines: VecDeque<String>,
    active_line: Vec<char>,
    cursor_col: usize,
    parse_state: AnsiParseState,
    saw_cursor_home: bool,
    utf8_tail: Vec<u8>,
}

impl SerialScreenBuffer {
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            completed_lines: VecDeque::with_capacity(cap),
            ..Self::default()
        }
    }

    pub fn push_completed_line(&mut self, line: String) {
        if self.completed_lines.len() >= 2000 {
            self.completed_lines.pop_front();
        }
        self.completed_lines.push_back(line);
    }

    pub fn clear_scrollback(&mut self) {
        self.completed_lines.clear();
    }

    pub fn snapshot(&self) -> SerialSnapshot {
        SerialSnapshot {
            lines: self.completed_lines.iter().cloned().collect(),
            active_line: self.active_line.iter().collect(),
            cursor_col: self.cursor_col,
        }
    }

    pub fn all_lines(&self) -> Vec<String> {
        let mut out: Vec<String> = self.completed_lines.iter().cloned().collect();
        if !self.active_line.is_empty() {
            out.push(self.active_line.iter().collect());
        }
        out
    }

    fn put_char(&mut self, ch: char) {
        while self.active_line.len() < self.cursor_col {
            self.active_line.push(' ');
        }
        if self.cursor_col < self.active_line.len() {
            self.active_line[self.cursor_col] = ch;
        } else {
            self.active_line.push(ch);
        }
        self.cursor_col += 1;
    }

    fn trim_erased_tail(&mut self) {
        while self.active_line.len() > self.cursor_col && self.active_line.last() == Some(&' ') {
            self.active_line.pop();
        }
    }

    fn handle_csi(&mut self, params: &str, final_byte: char, dsr_col: &mut Option<usize>) {
        let first_num = params
            .trim_start_matches('?')
            .split(';')
            .next()
            .and_then(|s| s.parse::<usize>().ok());

        match final_byte {
            'K' => match first_num.unwrap_or(0) {
                0 => {
                    if self.cursor_col < self.active_line.len() {
                        self.active_line.truncate(self.cursor_col);
                    }
                }
                1 => {
                    let end = self.cursor_col.min(self.active_line.len().saturating_sub(1));
                    for idx in 0..=end {
                        if idx < self.active_line.len() {
                            self.active_line[idx] = ' ';
                        }
                    }
                }
                2 => {
                    self.active_line.clear();
                    self.cursor_col = 0;
                }
                _ => {}
            },
            'J' => {
                let mode = first_num.unwrap_or(0);
                if mode == 2 || mode == 3 || self.saw_cursor_home {
                    self.completed_lines.clear();
                    self.active_line.clear();
                    self.cursor_col = 0;
                } else if self.cursor_col < self.active_line.len() {
                    self.active_line.truncate(self.cursor_col);
                }
                self.saw_cursor_home = false;
            }
            'H' | 'f' => {
                if params.is_empty() || params == "1;1" || params == ";" {
                    self.saw_cursor_home = true;
                }
            }
            'D' => {
                let step = first_num.unwrap_or(1).max(1);
                self.cursor_col = self.cursor_col.saturating_sub(step);
                self.trim_erased_tail();
            }
            'C' => {
                let step = first_num.unwrap_or(1).max(1);
                self.cursor_col += step;
                while self.active_line.len() < self.cursor_col {
                    self.active_line.push(' ');
                }
            }
            'G' => {
                let col1 = first_num.unwrap_or(1).max(1);
                self.cursor_col = col1 - 1;
                while self.active_line.len() < self.cursor_col {
                    self.active_line.push(' ');
                }
            }
            'P' => {
                let count = first_num.unwrap_or(1).max(1);
                for _ in 0..count {
                    if self.cursor_col < self.active_line.len() {
                        self.active_line.remove(self.cursor_col);
                    }
                }
            }
            '@' => {
                let count = first_num.unwrap_or(1).max(1);
                for _ in 0..count {
                    if self.cursor_col <= self.active_line.len() {
                        self.active_line.insert(self.cursor_col, ' ');
                    }
                }
            }
            'n' => {
                if params == "6" {
                    *dsr_col = Some(self.cursor_col + 1);
                }
            }
            _ => {}
        }
    }

    fn feed_char(&mut self, ch: char, completed: &mut Vec<String>, dsr_col: &mut Option<usize>) {
        match &mut self.parse_state {
            AnsiParseState::Escape => {
                match ch {
                    '[' => self.parse_state = AnsiParseState::Csi(String::new()),
                    ']' => self.parse_state = AnsiParseState::Osc,
                    'c' => {
                        self.active_line.clear();
                        self.cursor_col = 0;
                        self.parse_state = AnsiParseState::Ground;
                    }
                    _ => self.parse_state = AnsiParseState::Ground,
                }
                return;
            }
            AnsiParseState::Osc => {
                if ch == '\x07' || ch == '\n' {
                    self.parse_state = AnsiParseState::Ground;
                }
                return;
            }
            AnsiParseState::Csi(params) => {
                if ('@'..='~').contains(&ch) {
                    let params_owned = std::mem::take(params);
                    self.parse_state = AnsiParseState::Ground;
                    self.handle_csi(&params_owned, ch, dsr_col);
                } else if params.len() < 32 {
                    params.push(ch);
                } else {
                    self.parse_state = AnsiParseState::Ground;
                }
                return;
            }
            AnsiParseState::Ground => {}
        }

        if ch != '\x1b' {
            self.saw_cursor_home = false;
        }

        match ch {
            '\x1b' => {
                self.parse_state = AnsiParseState::Escape;
            }
            '\r' => {
                self.cursor_col = 0;
            }
            '\n' => {
                let line: String = self.active_line.iter().collect();
                self.push_completed_line(line.clone());
                completed.push(line);
                self.active_line.clear();
                self.cursor_col = 0;
            }
            '\x08' => {
                self.cursor_col = self.cursor_col.saturating_sub(1);
                self.trim_erased_tail();
            }
            '\t' => {
                let next_stop = ((self.cursor_col / 8) + 1) * 8;
                while self.cursor_col < next_stop {
                    self.put_char(' ');
                }
            }
            c if c >= ' ' => {
                self.put_char(c);
            }
            _ => {}
        }
    }

    pub fn feed_bytes(&mut self, bytes: &[u8]) -> (Vec<String>, Option<usize>) {
        self.utf8_tail.extend_from_slice(bytes);
        let mut completed = Vec::new();
        let mut dsr_col = None;

        loop {
            match std::str::from_utf8(&self.utf8_tail) {
                Ok(valid) => {
                    let owned = valid.to_string();
                    self.utf8_tail.clear();
                    for ch in owned.chars() {
                        self.feed_char(ch, &mut completed, &mut dsr_col);
                    }
                    break;
                }
                Err(err) => {
                    let valid_up_to = err.valid_up_to();
                    if valid_up_to > 0 {
                        let valid_part =
                            std::str::from_utf8(&self.utf8_tail[..valid_up_to]).unwrap().to_string();
                        for ch in valid_part.chars() {
                            self.feed_char(ch, &mut completed, &mut dsr_col);
                        }
                        self.utf8_tail.drain(..valid_up_to);
                    }
                    match err.error_len() {
                        Some(invalid_len) => {
                            self.utf8_tail.drain(..invalid_len);
                        }
                        None => break,
                    }
                }
            }
        }

        (completed, dsr_col)
    }
}

pub struct EmulatorSession {
    pub config: SessionConfig,
    pub qemu_bin: PathBuf,
    pub kernel_img: PathBuf,
    pub bridge_bin: PathBuf,
    child: Option<Child>,
    serial_stdin: Arc<Mutex<Option<ChildStdin>>>,
    serial_screen: Arc<Mutex<SerialScreenBuffer>>,
}

pub fn strip_ansi_and_control_chars(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    while let Some(c) = chars.next() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(c) = chars.next() {
                        if c == '\x07' {
                            break;
                        }
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            }
            continue;
        }
        if ch == '\t' || ch >= ' ' {
            out.push(ch);
        }
    }
    out
}

fn spawn_stdout_serial_reader<R: std::io::Read + Send + 'static>(
    mut stream: R,
    screen_ref: Arc<Mutex<SerialScreenBuffer>>,
    stdin_ref: Arc<Mutex<Option<ChildStdin>>>,
    log_path: PathBuf,
) {
    thread::spawn(move || {
        let mut log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .ok();
        let mut buf = [0u8; 1024];
        loop {
            match stream.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let (completed, dsr_col) = {
                        let mut guard = screen_ref.lock().unwrap();
                        guard.feed_bytes(&buf[..n])
                    };
                    if let Some(col) = dsr_col {
                        if let Some(ref mut stdin) = *stdin_ref.lock().unwrap() {
                            let reply = format!("\x1b[1;{}R", col);
                            let _ = stdin.write_all(reply.as_bytes());
                            let _ = stdin.flush();
                        }
                    }
                    if let Some(ref mut f) = log_file {
                        for line in completed {
                            let _ = writeln!(f, "{}", line);
                        }
                    }
                }
            }
        }
    });
}

fn spawn_stderr_serial_reader<R: std::io::Read + Send + 'static>(
    stream: R,
    screen_ref: Arc<Mutex<SerialScreenBuffer>>,
    log_path: PathBuf,
) {
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        let mut log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .ok();
        for line in reader.lines().map_while(Result::ok) {
            let clean = strip_ansi_and_control_chars(line.trim_end_matches('\r'));
            if let Some(ref mut f) = log_file {
                let _ = writeln!(f, "[stderr] {}", clean);
            }
            let mut guard = screen_ref.lock().unwrap();
            guard.push_completed_line(format!("[qemu] {}", clean));
        }
    });
}

impl EmulatorSession {
    pub fn start(config: SessionConfig) -> Result<Self, String> {
        Self::start_with_overrides(config, None, None)
    }

    pub fn start_with_overrides(
        config: SessionConfig,
        custom_qemu: Option<&Path>,
        custom_kernel: Option<&Path>,
    ) -> Result<Self, String> {
        let qemu_bin = match custom_qemu {
            Some(p) if p.is_file() => p.to_path_buf(),
            _ => resolve_qemu_binary(&config.root_dir)?,
        };
        let kernel_img = match custom_kernel {
            Some(p) if p.is_file() => p.to_path_buf(),
            _ => resolve_kernel_image(&config.root_dir)?,
        };
        let bridge_src = resolve_bridge_binary(&config.root_dir)?;

        if !config.drive_img.is_file() {
            return Err(format!(
                "Guest Image does not exist: {}",
                config.drive_img.display()
            ));
        }

        let work_dir = config.root_dir.join("work");
        let _ = fs::create_dir_all(&work_dir);
        let staged_bridge = work_dir.join(format!("qemu-hw-bridge-{}.bin", config.board));
        let _ = fs::copy(&bridge_src, &staged_bridge);
        let bridge_bin = if staged_bridge.is_file() {
            staged_bridge
        } else {
            bridge_src
        };
        let serial_log_path = work_dir.join(format!("{}-serial.log", config.board));
        let _ = File::create(&serial_log_path);

        let bind_ip = if config.bind_all {
            "0.0.0.0"
        } else {
            "127.0.0.1"
        };
        let mut hostfwd = format!(
            "user,id=net0,hostfwd=tcp:{}:{}-:8091,hostfwd=tcp:{}:{}-:5555",
            bind_ip, config.bridge_port, bind_ip, config.adb_port
        );
        if let Some(sdk_p) = config.sdk_port {
            hostfwd.push_str(&format!(",hostfwd=tcp:{}:{}-:8090", bind_ip, sdk_p));
        }

        let images_dir = config
            .drive_img
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(&work_dir);

        let data_img_opt = if config.persist_data {
            Some(crate::image_builder::ensure_persistent_data_image(
                images_dir,
                &config.board,
                false,
            )?)
        } else {
            None
        };

        let data_cmdline = if data_img_opt.is_some() {
            " quasar.data=persist"
        } else if !config.snapshot {
            " quasar.data=rootfs"
        } else {
            ""
        };

        let append_cmdline = format!(
            "console=ttyAMA0 root=/dev/vda rw ip=dhcp init=/quasar-qemu-init.sh quasar.board={} quasar.mode={}{} androidboot.serialno=EMU0001 panic=-1",
            config.board_cmdline, config.mode, data_cmdline
        );

        let root_drive_arg = if config.snapshot && data_img_opt.is_some() {
            format!(
                "file={},if=virtio,format=raw,snapshot=on",
                config.drive_img.display()
            )
        } else {
            format!("file={},if=virtio,format=raw", config.drive_img.display())
        };

        // TCG bench: the guest needs real parallelism (Luna + plugin + audio).
        // 4 vCPUs on the 8-thread host is the sweet spot; override if needed.
        let host_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .max(1);
        let smp = std::env::var("KUYASE_SMP")
            .ok()
            .and_then(|s| s.trim().parse::<usize>().ok())
            .map(|n| n.clamp(1, host_threads))
            .unwrap_or_else(|| host_threads.clamp(2, 4))
            .to_string();
        let mem = std::env::var("KUYASE_MEM").unwrap_or_else(|_| "1G".to_string());
        let mut cmd = Command::new(&qemu_bin);
        cmd.args([
            "-M",
            "virt",
            "-cpu",
            "cortex-a53",
            "-smp",
            smp.as_str(),
            "-m",
            mem.as_str(),
            "-kernel",
        ])
        .arg(&kernel_img)
        .arg("-append")
        .arg(&append_cmdline)
        .arg("-drive")
        .arg(&root_drive_arg)
        .arg("-drive")
        .arg(format!(
            "file={},if=virtio,format=raw,readonly=on",
            bridge_bin.display()
        ));

        if let Some(ref data_img) = data_img_opt {
            cmd.arg("-drive")
                .arg(format!("file={},if=virtio,format=raw", data_img.display()));
        } else if config.snapshot {
            cmd.arg("-snapshot");
        }

        cmd.arg("-netdev")
            .arg(&hostfwd)
            .args(["-device", "virtio-net-pci,netdev=net0"]);

        let mut lan_warning: Option<String> = None;
        if let Some(ref tap_raw) = config.lan_tap {
            let tap = tap_raw.trim();
            if !tap.is_empty() {
                let tap_available = if cfg!(target_os = "linux") {
                    Path::new(&format!("/sys/class/net/{}", tap)).exists()
                } else {
                    true
                };
                if tap_available {
                    let mac = config
                        .lan_mac
                        .as_deref()
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| default_lan_mac(config.port_offset));
                    let tap_netdev = if cfg!(windows) {
                        format!("tap,ifname={},id=lan0", tap)
                    } else {
                        format!("tap,ifname={},script=no,downscript=no,id=lan0", tap)
                    };
                    cmd.arg("-netdev")
                        .arg(tap_netdev)
                        .arg("-device")
                        .arg(format!("virtio-net-pci,netdev=lan0,mac={}", mac));
                } else {
                    let msg = format!(
                        "[kuyase] LAN tap {} not available; user networking only",
                        tap
                    );
                    println!("{}", msg);
                    lan_warning = Some(msg);
                }
            }
        }

        cmd.args(["-serial", "stdio", "-display", "none"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            format!(
                "failed to spawn qemu-system-aarch64 ('{}'): {}",
                qemu_bin.display(),
                e
            )
        })?;

        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let serial_stdin = Arc::new(Mutex::new(stdin));
        let serial_screen = Arc::new(Mutex::new(SerialScreenBuffer::with_capacity(2048)));
        if let Some(warn) = lan_warning {
            serial_screen.lock().unwrap().push_completed_line(warn);
        }

        if let Some(out) = stdout {
            spawn_stdout_serial_reader(
                out,
                serial_screen.clone(),
                serial_stdin.clone(),
                serial_log_path.clone(),
            );
        }

        if let Some(err) = stderr {
            spawn_stderr_serial_reader(err, serial_screen.clone(), serial_log_path);
        }

        Ok(Self {
            config,
            qemu_bin,
            kernel_img,
            bridge_bin,
            child: Some(child),
            serial_stdin,
            serial_screen,
        })
    }

    pub fn send_serial_input(&self, input: &str) -> Result<(), String> {
        let mut guard = self.serial_stdin.lock().unwrap();
        let Some(ref mut stdin) = *guard else {
            return Err("serial stdin is closed".to_string());
        };
        // ARM PL011 UART (ttyAMA0) has a 16/32-byte hardware RX FIFO.
        // Pace writes in <=12-byte chunks aligned to UTF-8 char boundaries
        // so multi-byte Unicode characters are never split across writes.
        let mut chunk = String::with_capacity(16);
        let mut chunk_idx = 0usize;
        for ch in input.chars() {
            if !chunk.is_empty() && chunk.len() + ch.len_utf8() > 12 {
                if chunk_idx > 0 {
                    thread::sleep(Duration::from_millis(4));
                }
                stdin
                    .write_all(chunk.as_bytes())
                    .and_then(|_| stdin.flush())
                    .map_err(|e| format!("failed to write to serial stdin: {}", e))?;
                chunk.clear();
                chunk_idx += 1;
            }
            chunk.push(ch);
        }
        if !chunk.is_empty() {
            if chunk_idx > 0 {
                thread::sleep(Duration::from_millis(4));
            }
            stdin
                .write_all(chunk.as_bytes())
                .and_then(|_| stdin.flush())
                .map_err(|e| format!("failed to write to serial stdin: {}", e))?;
        }
        Ok(())
    }

    pub fn clear_serial_lines(&self) {
        self.serial_screen.lock().unwrap().clear_scrollback();
    }

    pub fn serial_lines(&self) -> Vec<String> {
        self.serial_screen.lock().unwrap().all_lines()
    }

    pub fn serial_snapshot(&self) -> SerialSnapshot {
        self.serial_screen.lock().unwrap().snapshot()
    }

    pub fn is_running(&mut self) -> bool {
        let Some(ref mut child) = self.child else {
            return false;
        };
        matches!(child.try_wait(), Ok(None))
    }

    pub fn stop(&mut self) {
        *self.serial_stdin.lock().unwrap() = None;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Ok(Some(_)) = child.try_wait() {
                    break;
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                thread::sleep(Duration::from_millis(25));
            }
        }
    }
}

impl Drop for EmulatorSession {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_board_and_image_preserves_canonical_slug_and_nc_variants() {
        let tmp = std::env::temp_dir().join(format!(
            "kuyase_resolve_board_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        let work = tmp.join("work");
        fs::create_dir_all(&work).unwrap();

        let mini2_img = work.join("yandexmini2-system.ext4");
        let mini2nc_img = work.join("yandexmini2nc-system.ext4");
        let plumnc_img = work.join("plumnc-system.ext4");
        let alias_img = work.join("mini1-system.ext4");
        fs::write(&mini2_img, b"dummy").unwrap();
        fs::write(&mini2nc_img, b"dummy").unwrap();
        fs::write(&plumnc_img, b"dummy").unwrap();
        fs::write(&alias_img, b"dummy").unwrap();

        // Exact nc variants resolve to their own image and canonical slug
        let (slug, path) = resolve_board_and_image(&tmp, Some("yandexmini2nc")).unwrap();
        assert_eq!(slug, "yandexmini2nc");
        assert_eq!(path, mini2nc_img);

        let (slug, path) = resolve_board_and_image(&tmp, Some("plumnc")).unwrap();
        assert_eq!(slug, "plumnc");
        assert_eq!(path, plumnc_img);

        // Aliases for nc variants also resolve to the canonical slug and image
        let (slug, path) = resolve_board_and_image(&tmp, Some("mini2nc")).unwrap();
        assert_eq!(slug, "yandexmini2nc");
        assert_eq!(path, mini2nc_img);

        let (slug, path) = resolve_board_and_image(&tmp, Some("prune")).unwrap();
        assert_eq!(slug, "plumnc");
        assert_eq!(path, plumnc_img);

        // An image named after an alias still resolves to a canonical board_slug
        let (slug, path) = resolve_board_and_image(&tmp, Some("mini1")).unwrap();
        assert_eq!(slug, "yandexmini");
        assert_eq!(path, alias_img);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_serial_screen_buffer_native_sh_prompt_tab_and_editing() {
        let mut screen = SerialScreenBuffer::with_capacity(64);

        // 1. BusyBox ash prints prompt without trailing newline and queries DSR \x1b[6n
        let (done, dsr) = screen.feed_bytes(b"root@qemu-orion:/# \x1b[6n");
        assert!(done.is_empty());
        assert_eq!(dsr, Some(20));
        let snap = screen.snapshot();
        assert_eq!(snap.active_line, "root@qemu-orion:/# ");
        assert_eq!(snap.cursor_col, 19);

        // 2. Typing "cd /sys/cl" and BusyBox ash completing "ass/" on Tab
        screen.feed_bytes(b"cd /sys/cl");
        screen.feed_bytes(b"ass/");
        assert_eq!(screen.snapshot().active_line, "root@qemu-orion:/# cd /sys/class/");

        // 3. Backspace (\x08 \x08) erasing '/'
        screen.feed_bytes(b"\x08 \x08");
        assert_eq!(screen.snapshot().active_line, "root@qemu-orion:/# cd /sys/class");

        // 4. Multi-candidate Tab output (\r\n + candidates + \r\n + reprinted prompt)
        let (done2, _) = screen.feed_bytes(
            b"\r\nbacklight/  graphics/  leds/\r\nroot@qemu-orion:/# cd /sys/class/",
        );
        assert_eq!(
            done2,
            vec![
                "root@qemu-orion:/# cd /sys/class".to_string(),
                "backlight/  graphics/  leds/".to_string()
            ]
        );
        assert_eq!(screen.snapshot().active_line, "root@qemu-orion:/# cd /sys/class/");

        // 5. History recall (ArrowUp) replacing long command with shorter command via \r + \x1b[K
        screen.feed_bytes(b"\rroot@qemu-orion:/# ps\x1b[K");
        assert_eq!(screen.snapshot().active_line, "root@qemu-orion:/# ps");
        assert_eq!(screen.snapshot().cursor_col, 21);
    }
}
