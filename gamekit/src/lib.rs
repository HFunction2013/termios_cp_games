//! gamekit —— 跨平台终端游戏基础库
//!
//! 用 Rust + crossterm 封装了四个游戏共用的终端能力：
//! 清屏、隐藏/显示光标、设置窗口标题、延时、原始模式（实时按键）、
//! 阻塞式按键读取、文本行读取以及 ANSI 颜色输出。
//! 全部为跨平台实现（Windows / macOS / Linux）。

use std::io::{self, Write};
use std::time::Duration;

pub use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

/// 初始化终端（Windows 下启用 ANSI 虚拟终端支持；其他平台无操作）。
/// 在输出任何 ANSI 转义序列之前调用一次。
pub fn init() {
    #[cfg(windows)]
    {
        let _ = crossterm::ansi_support::enable_ansi_support();
    }
    let _ = io::stdout().flush();
}

/// 清屏并把光标移到左上角（跨平台 ANSI 序列）。
pub fn clear_screen() {
    let _ = io::stdout().write_all(b"\x1b[2J\x1b[H");
    let _ = io::stdout().flush();
}

/// 隐藏光标。
pub fn hide_cursor() {
    let _ = io::stdout().write_all(b"\x1b[?25l");
    let _ = io::stdout().flush();
}

/// 显示光标。
pub fn show_cursor() {
    let _ = io::stdout().write_all(b"\x1b[?25h");
    let _ = io::stdout().flush();
}

/// 设置终端窗口标题（ANSI OSC 序列，Windows 10+ / macOS / Linux 均支持）。
pub fn set_title(title: &str) {
    let _ = io::stdout().write_all(format!("\x1b]0;{}\x07", title).as_bytes());
    let _ = io::stdout().flush();
}

/// 延时（毫秒）。
pub fn sleep_ms(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

// ---------------------------------------------------------------------------
// 颜色输出（ANSI 16 色，模拟 Windows 控制台颜色）
// ---------------------------------------------------------------------------
pub mod color {
    pub const RESET: &str = "\x1b[0m";
    // 黑色（暗）
    pub const BLACK: &str = "\x1b[30m";
    // 亮白
    pub const WHITE: &str = "\x1b[97m";
    // 灰色（Windows 8 号色：深灰）
    pub const GRAY: &str = "\x1b[90m";
    // 亮红
    pub const RED: &str = "\x1b[91m";
    // 亮绿
    pub const GREEN: &str = "\x1b[92m";
    // 亮黄
    pub const YELLOW: &str = "\x1b[93m";
    // 亮蓝
    pub const BLUE: &str = "\x1b[94m";
    // 亮品红
    pub const MAGENTA: &str = "\x1b[95m";
    // 亮青
    pub const CYAN: &str = "\x1b[96m";
    // 暗红
    pub const DARK_RED: &str = "\x1b[31m";
    // 暗绿
    pub const DARK_GREEN: &str = "\x1b[32m";
    // 暗黄
    pub const DARK_YELLOW: &str = "\x1b[33m";
    // 暗蓝
    pub const DARK_BLUE: &str = "\x1b[34m";

    /// 按 Windows 控制台色号（0~15）输出对应的 ANSI 颜色码。
    /// 用于保持与原版“破解版”着色逻辑一致。
    pub fn win_color(n: u8) -> &'static str {
        match n {
            0 => "\x1b[30m",   // 黑
            1 => "\x1b[34m",   // 暗蓝
            2 => "\x1b[32m",   // 暗绿
            3 => "\x1b[36m",   // 暗青
            4 => "\x1b[31m",   // 暗红
            5 => "\x1b[35m",   // 暗品红
            6 => "\x1b[33m",   // 暗黄
            7 => "\x1b[37m",   // 亮灰
            8 => "\x1b[90m",   // 深灰
            9 => "\x1b[94m",   // 亮蓝
            10 => "\x1b[92m",  // 亮绿
            11 => "\x1b[96m",  // 亮青
            12 => "\x1b[91m",  // 亮红
            13 => "\x1b[95m",  // 亮品红
            14 => "\x1b[93m",  // 亮黄
            15 => "\x1b[97m",  // 亮白
            _ => "\x1b[0m",
        }
    }
}

// ---------------------------------------------------------------------------
// 原始模式（实时按键）
// ---------------------------------------------------------------------------

/// 原始模式 RAII 守卫：进入原始模式，离开作用域时自动恢复并显示光标。
pub struct RawMode;

impl RawMode {
    pub fn enter() -> io::Result<RawMode> {
        // 开启原始模式
        crossterm::terminal::enable_raw_mode()?;
        // 确保清屏、光标等命令可用
        let _ = crossterm::execute!(io::stdout(), crossterm::terminal::Clear(crossterm::terminal::ClearType::All));
        let _ = io::stdout().flush();
        Ok(RawMode)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = io::stdout().write_all(b"\x1b[?25h");
        let _ = io::stdout().flush();
    }
}

/// 每个游戏帧内采集到的按键集合（按下即置位，帧结束后清零）。
#[derive(Default, Clone, Copy)]
pub struct FrameKeys {
    pub w: bool,
    pub s: bool,
    pub a: bool,
    pub d: bool,
    pub q: bool,
    pub key1: bool,
    pub key2: bool,
    pub key3: bool,
    pub y: bool,
    pub n: bool,
    pub enter: bool,
    pub backspace: bool,
}

impl FrameKeys {
    /// 是否有任意方向键被按下（W/S/A/D 之一）。
    pub fn any_move(&self) -> bool {
        self.w || self.s || self.a || self.d
    }
}

fn set_flag(k: &mut FrameKeys, code: KeyCode) {
    match code {
        KeyCode::Char(c) => match c.to_ascii_lowercase() {
            'w' => k.w = true,
            's' => k.s = true,
            'a' => k.a = true,
            'd' => k.d = true,
            'q' => k.q = true,
            'y' => k.y = true,
            'n' => k.n = true,
            '1' => k.key1 = true,
            '2' => k.key2 = true,
            '3' => k.key3 = true,
            _ => {}
        },
        KeyCode::Enter => k.enter = true,
        KeyCode::Backspace => k.backspace = true,
        _ => {}
    }
}

/// 非阻塞采集当前所有待处理按键（一帧调用一次，用于实时移动游戏）。
/// 只要本帧内收到某键的 Press / Repeat 事件，对应标志即被置位。
pub fn drain_keys() -> FrameKeys {
    let mut keys = FrameKeys::default();
    loop {
        match crossterm::event::poll(Duration::ZERO) {
            Ok(true) => {
                match crossterm::event::read() {
                    Ok(Event::Key(ke)) => {
                        if matches!(ke.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                            set_flag(&mut keys, ke.code);
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            _ => break,
        }
    }
    keys
}

/// 阻塞读取一个按键（用于存档命名、菜单选择、Y/N 确认等交互）。
pub fn read_key() -> Option<KeyCode> {
    loop {
        match crossterm::event::read() {
            Ok(Event::Key(ke)) => {
                if matches!(ke.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                    return Some(ke.code);
                }
            }
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
}

/// 阻塞读取一个字符按键（字母/数字），用于原版的“逐字输入”。
/// 返回 None 表示按下了回车（结束输入）。
pub fn read_char_or_enter() -> Option<char> {
    loop {
        match crossterm::event::read() {
            Ok(Event::Key(ke)) => {
                if matches!(ke.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                    match ke.code {
                        KeyCode::Enter => return None,
                        KeyCode::Char(c) => {
                            if c.is_alphanumeric() {
                                return Some(c);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
}

/// 阻塞读取并返回完整文本行（用于存档命名等）。
/// 支持退格删除，回车结束。返回的字符串不含回车。
pub fn read_line_raw(prompt: &str) -> String {
    let mut s = String::new();
    loop {
        match crossterm::event::read() {
            Ok(Event::Key(ke)) => {
                if matches!(ke.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                    match ke.code {
                        KeyCode::Enter => return s,
                        KeyCode::Backspace => {
                            if !s.is_empty() {
                                s.pop();
                                let _ = io::stdout().write_all(b"\r");
                                let _ = io::stdout().write_all(prompt.as_bytes());
                                let _ = io::stdout().write_all(s.as_bytes());
                                let _ = io::stdout().write_all(b"   ");
                                let _ = io::stdout().flush();
                            }
                        }
                        KeyCode::Char(c) => {
                            s.push(c);
                            let _ = io::stdout().write_all(&[c as u8]);
                            let _ = io::stdout().flush();
                        }
                        _ => {}
                    }
                }
            }
            Ok(_) => continue,
            Err(_) => return s,
        }
    }
}

/// 非原始模式下读取一行文本（用于扫雷、中国象棋的回合制输入）。
/// EOF 时返回 None。
pub fn read_line_cooked() -> Option<String> {
    let mut line = String::new();
    match io::stdin().read_line(&mut line) {
        Ok(0) => None,
        Ok(_) => Some(line.trim_end_matches(['\r', '\n']).to_string()),
        Err(_) => None,
    }
}

// ---------------------------------------------------------------------------
// 鼠标交互模式
// ---------------------------------------------------------------------------

/// 一次鼠标点击（按下瞬间的坐标与按键）。
/// `column` / `row` 为 0 基坐标（左上角为 (0,0)，与光标位置一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseClick {
    pub button: MouseButton,
    pub column: u16,
    pub row: u16,
}

/// 鼠标模式下读取到的一个交互事件：按键或鼠标点击。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    Key(KeyCode),
    Mouse(MouseClick),
}

/// 鼠标交互模式 RAII 守卫：进入原始模式并启用鼠标捕获，
/// 离开作用域时自动禁用鼠标捕获、恢复原始终端并显示光标。
pub struct MouseMode;

impl MouseMode {
    pub fn enter() -> io::Result<MouseMode> {
        // 开启原始模式
        crossterm::terminal::enable_raw_mode()?;
        // 启用鼠标捕获
        let _ = crossterm::execute!(
            io::stdout(),
            crossterm::event::EnableMouseCapture,
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
        );
        let _ = io::stdout().flush();
        Ok(MouseMode)
    }
}

impl Drop for MouseMode {
    fn drop(&mut self) {
        let _ = crossterm::execute!(io::stdout(), crossterm::event::DisableMouseCapture);
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = io::stdout().write_all(b"\x1b[?25h");
        let _ = io::stdout().flush();
    }
}

/// 阻塞读取一个按键或鼠标点击（仅响应按下类事件）。
/// 鼠标按下返回 `InputEvent::Mouse`，按键按下返回 `InputEvent::Key`。
/// 出错 / EOF 时返回 None。
pub fn read_key_or_mouse() -> Option<InputEvent> {
    loop {
        match crossterm::event::read() {
            Ok(Event::Key(ke)) => {
                if matches!(ke.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                    return Some(InputEvent::Key(ke.code));
                }
            }
            Ok(Event::Mouse(me)) => {
                if let MouseEventKind::Down(button) = me.kind {
                    return Some(InputEvent::Mouse(MouseClick {
                        button,
                        column: me.column,
                        row: me.row,
                    }));
                }
            }
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
}


/// 在原始模式下读取单个按键并判断是否为 Y/N（用于“是否回档”等提示）。
/// 返回 'Y' 或 'N'；EOF/出错时返回 'N'。
pub fn read_yn() -> char {
    loop {
        match crossterm::event::read() {
            Ok(Event::Key(ke)) => {
                if matches!(ke.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                    match ke.code {
                        KeyCode::Char(c) if c.to_ascii_lowercase() == 'y' => return 'Y',
                        KeyCode::Char(c) if c.to_ascii_lowercase() == 'n' => return 'N',
                        _ => {}
                    }
                }
            }
            Ok(_) => continue,
            Err(_) => return 'N',
        }
    }
}

/// 阻塞读取一个数字键（0~9），用于存档选择。
pub fn read_digit() -> Option<u32> {
    loop {
        match crossterm::event::read() {
            Ok(Event::Key(ke)) => {
                if matches!(ke.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                    match ke.code {
                        KeyCode::Char(c) if c.is_ascii_digit() => return c.to_digit(10),
                        KeyCode::Enter => return None,
                        _ => {}
                    }
                }
            }
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
}

pub fn detect_terminal_theme() -> bool {
    use crossterm::terminal;
    use std::io::{Read, Write};
    use std::time::Duration;

    // 启用原始模式
    let _ = terminal::enable_raw_mode();
    
    // 查询终端背景色
    print!("\x1b]11;?\x07");
    std::io::stdout().flush().unwrap();
    
    // 读取响应
    let mut response = String::new();
    let mut buf = [0u8; 1];
    let start = std::time::Instant::now();
    
    while start.elapsed() < Duration::from_millis(200) {
        if let Ok(n) = std::io::stdin().read(&mut buf) {
            if n > 0 {
                response.push(buf[0] as char);
                if response.contains("\x1b\\") || response.contains('\x07') {
                    break;
                }
            }
        }
    }
    
    // 恢复原始模式
    let _ = terminal::disable_raw_mode();
    
    // 解析颜色值
    if let Some(rgb_part) = response.split("rgb:").nth(1) {
        let rgb_part = rgb_part.trim_end_matches(['\x1b', '\\', '\x07', '\n', '\r']);
        let parts: Vec<&str> = rgb_part.split('/').collect();
        
        if parts.len() >= 3 {
            // 处理 2 位或 4 位十六进制
            let parse_hex = |s: &str| -> u8 {
                let s = s.trim();
                if s.len() == 4 {
                    // 4位十六进制（16位颜色），取高8位
                    u8::from_str_radix(&s[0..2], 16).unwrap_or(0)
                } else {
                    // 2位十六进制（8位颜色）
                    u8::from_str_radix(s, 16).unwrap_or(0)
                }
            };
            
            let r = parse_hex(parts[0]);
            let g = parse_hex(parts[1]);
            let b = parse_hex(parts[2]);
            
            // 计算亮度（ITU-R BT.601）
            let luminance = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
            return luminance > 128.0; // 亮色背景返回 true
        }
    }
    
    // 默认假设为深色背景
    false
}