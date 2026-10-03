//! 扫雷 —— 原 C++（Windows 版）的 Rust 跨平台移植
//!
//! 玩法与原版完全一致：
//! - 10×10 棋盘，难度 1~5（雷数 = 10 × 难度）
//! - 输入“行 列”翻开格子，行/列后加 p 表示标记地雷，如：1 2p
//! - 全部非雷格子翻开即胜利
//!
//! 交互方式：
//! - 默认：鼠标交互 —— 左键翻开、右键标记/取消标记、q 退出；
//! - 传 --legacy 参数则退回原始的“输入坐标”文字交互（行/列后加 p 标记）。
//! - 两种模式均支持 q 退出（原版 README 声称 q 退出但代码未实现，现已修复）。
//!
//! 已用 ANSI 转义序列 + 标准输入替代 system("cls") / windows.h，
//! 可在 Windows / macOS / Linux 终端中运行。

use gamekit::{InputEvent, KeyCode, MouseButton, MouseMode};
use std::time::{SystemTime, UNIX_EPOCH};

const N: usize = 10;

/// 简易确定性伪随机数（xorshift64*），避免额外依赖。
struct Rng(u64);

impl Rng {
    fn from_time() -> Rng {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15);
        Rng(nanos | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    /// 返回 [lo, hi] 区间内的随机数
    fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + (self.next_u64() as usize % (hi - lo + 1))
    }
}

/// 八个方向（原版 c[1..=8]）
const DIR: [(i32, i32); 8] = [
    (-1, -1),
    (-1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
    (1, 0),
    (1, -1),
    (0, -1),
];

struct Game {
    /// 实际地雷分布（'0'~'8'、'*'）
    a: [[u8; 15]; 15],
    /// 玩家可见棋盘（' '、'P'、'0'~'8'、'*'）
    b: [[u8; 15]; 15],
    /// 游戏是否存活（踩雷后为 false）
    alive: bool,
    /// 剩余未翻开的格子数
    m: i32,
    rng: Rng,
}

impl Game {
    fn new() -> Game {
        Game {
            a: [['0' as u8; 15]; 15],
            b: [[' ' as u8; 15]; 15],
            alive: true,
            m: 0,
            rng: Rng::from_time(),
        }
    }

    /// 放置一颗地雷（原版 f(n)）
    fn place_mine(&mut self, n: usize) {
        let mut x = self.rng.range(1, n);
        let mut y = self.rng.range(1, n);
        while self.a[x][y] == b'*' {
            x = self.rng.range(1, n);
            y = self.rng.range(1, n);
        }
        self.a[x][y] = b'*';
        for &(dx, dy) in &DIR {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if self.a[nx as usize][ny as usize] != b'*' {
                self.a[nx as usize][ny as usize] += 1;
            }
        }
    }

    /// 翻开 (i,j) 格子，返回本次翻开的格子数（原版 f1）
    fn reveal(&mut self, i: usize, j: usize) -> i32 {
        if self.b[i][j] != b' ' && self.b[i][j] != b'P' {
            return 0;
        }
        self.b[i][j] = self.a[i][j];
        if self.b[i][j] == b'*' {
            self.alive = false;
            0
        } else if self.b[i][j] == b'0' {
            let mut s = 1;
            for &(dx, dy) in &DIR {
                let ni = i as i32 + dx;
                let nj = j as i32 + dy;
                if ni >= 1
                    && ni <= N as i32
                    && nj >= 1
                    && nj <= N as i32
                    && self.b[ni as usize][nj as usize] == b' '
                {
                    s += self.reveal(ni as usize, nj as usize);
                }
            }
            s
        } else {
            1
        }
    }

    /// 打印棋盘（含标题栏）—— legacy（文字输入）模式使用，行为与原版一致。
    fn print_board(&self, show_mines: bool) {
        let sep = "\n   +-+-+-+-+-+-+-+-+-+-+\n";
        print!("    1 2 3 4 5 6 7 8 9 10 ");
        print!("{}", sep);
        for i in 1..=N {
            print!("{:2} |", i);
            for j in 1..=N {
                if show_mines && self.a[i][j] == b'*' {
                    print!("*|");
                } else {
                    print!("{}|", self.b[i][j] as char);
                }
            }
            print!("{}", sep);
        }
    }

    /// 生成整张棋盘字符串（\r\n 行尾，鼠标模式下直接打印）。
    /// 布局固定：第 i 行内容在 0 基第 2i 行、第 j 列内容在 0 基第 2+2j 列。
    fn board_string(&self, show_mines: bool) -> String {
        let mut s = String::new();
        s.push_str("    1 2 3 4 5 6 7 8 9 10 \r\n");
        s.push_str("   +-+-+-+-+-+-+-+-+-+-+\r\n");
        for i in 1..=N {
            s.push_str(&format!("{:2} |", i));
            for j in 1..=N {
                if show_mines && self.a[i][j] == b'*' {
                    s.push('*');
                } else {
                    s.push(self.b[i][j] as char);
                }
                s.push('|');
            }
            s.push_str("\r\n");
            s.push_str("   +-+-+-+-+-+-+-+-+-+-+\r\n");
        }
        s
    }
}

/// 把终端点击坐标（0 基，与光标位置一致）映射到棋盘格 (i, j)（1 基）。
/// 棋盘第 i 行内容在 0 基第 2i 行，第 j 列内容在 0 基第 2+2j 列；点中格线返回 None。
fn cell_from_click(column: u16, row: u16) -> Option<(usize, usize)> {
    let c = column as i32;
    let r = row as i32;
    if (c - 2) % 2 != 0 || r % 2 != 0 {
        return None;
    }
    let j = ((c - 2) / 2) as usize;
    let i = (r / 2) as usize;
    if i >= 1 && i <= N && j >= 1 && j <= N {
        Some((i, j))
    } else {
        None
    }
}

/// 输入是否为退出指令（q / Q）
fn is_quit(s: &str) -> bool {
    s.trim().eq_ignore_ascii_case("q")
}

fn main() {
    gamekit::init();
    let legacy = std::env::args().any(|a| a == "--legacy");
    let mut game = Game::new();
    println!("欢迎来到扫雷游戏");
    if legacy {
        game.run_legacy();
    } else {
        game.run_mouse();
    }
}

impl Game {
    /// 原版文字输入玩法（与移植前行为一致，额外修复 q 退出）。
    fn run_legacy(&mut self) {
        let mut k: usize;

        while self.alive {
            // 选择难度
            loop {
                print!("请选择难度(1~5):");
                use std::io::Write;
                let _ = std::io::stdout().flush();
                match gamekit::read_line_cooked() {
                    Some(line) => {
                        if is_quit(&line) {
                            return; // q 退出
                        }
                        match line.trim().parse::<i32>() {
                            Ok(v) => {
                                k = v.max(1).min(5) as usize;
                                if v > 5 {
                                    println!("已自动将难度设为5");
                                }
                                if v < 1 {
                                    println!("已自动将难度设为1");
                                }
                                break;
                            }
                            Err(_) => {
                                println!("无效输入，已自动将难度设为1");
                                k = 1;
                                break;
                            }
                        }
                    }
                    None => return, // EOF 退出
                }
            }

            // 初始化棋盘
            for i in 1..=N {
                for j in 1..=N {
                    self.a[i][j] = b'0';
                    self.b[i][j] = b' ';
                }
            }
            // 放置 n*k 颗雷
            for _ in 0..N * k {
                self.place_mine(N);
            }
            self.m = (N * N) as i32;
            self.alive = true;

            // 游戏主循环
            while self.alive && self.m > (N * k) as i32 {
                gamekit::clear_screen();
                self.print_board(false);

                print!("请输入坐标(在坐标后加上p表示标记,如 1 2p):");
                use std::io::Write;
                let _ = std::io::stdout().flush();

                let Some(line) = gamekit::read_line_cooked() else {
                    return;
                };
                if is_quit(&line) {
                    return; // q 退出
                }
                let tokens: Vec<&str> = line.split_whitespace().collect();
                if tokens.len() < 2 {
                    println!("无效的坐标!");
                    gamekit::sleep_ms(1000);
                    continue;
                }
                let x: usize = match tokens[0].parse() {
                    Ok(v) => v,
                    Err(_) => {
                        println!("无效的坐标!");
                        gamekit::sleep_ms(1000);
                        continue;
                    }
                };
                // 支持“行 列p”语法：p 紧跟在列坐标后（如 1 2p），
                // 也支持“行 列 p”语法（p 单独成词）
                let mut has_p = false;
                let y_str = if let Some(stripped) = tokens[1].strip_suffix(['p', 'P']) {
                    has_p = true;
                    stripped
                } else {
                    tokens[1]
                };
                let y: usize = match y_str.parse() {
                    Ok(v) => v,
                    Err(_) => {
                        println!("无效的坐标!");
                        gamekit::sleep_ms(1000);
                        continue;
                    }
                };

                if x > N || y > N || x < 1 || y < 1 {
                    println!("无效的坐标!");
                    gamekit::sleep_ms(1000);
                    continue;
                }

                // 剩余内容中是否含有 p（标记）
                if !has_p {
                    has_p = tokens.get(2).map(|t| t.contains('p') || t.contains('P')).unwrap_or(false);
                }
                if !has_p {
                    self.m -= self.reveal(x, y);
                } else {
                    if self.b[x][y] == b' ' {
                        self.b[x][y] = b'P';
                    } else {
                        println!("无法标记");
                        gamekit::sleep_ms(1000);
                    }
                }
            }

            // 显示最终棋盘（地雷显示为 *）
            gamekit::clear_screen();
            self.print_board(true);
        }

        print!("按回车键结束");
        use std::io::Write;
        let _ = std::io::stdout().flush();
        let _ = gamekit::read_line_cooked();
    }

    /// 鼠标交互玩法：左键翻开、右键标记/取消标记、q 退出。
    fn run_mouse(&mut self) {
        let mut k: usize;

        'outer: while self.alive {
            // 选择难度（键盘输入，q 退出）
            loop {
                print!("请选择难度(1~5):");
                use std::io::Write;
                let _ = std::io::stdout().flush();
                match gamekit::read_line_cooked() {
                    Some(line) => {
                        if is_quit(&line) {
                            return;
                        }
                        match line.trim().parse::<i32>() {
                            Ok(v) => {
                                k = v.max(1).min(5) as usize;
                                if v > 5 {
                                    println!("已自动将难度设为5");
                                }
                                if v < 1 {
                                    println!("已自动将难度设为1");
                                }
                                break;
                            }
                            Err(_) => {
                                println!("无效输入，已自动将难度设为1");
                                k = 1;
                                break;
                            }
                        }
                    }
                    None => return,
                }
            }

            // 初始化棋盘
            for i in 1..=N {
                for j in 1..=N {
                    self.a[i][j] = b'0';
                    self.b[i][j] = b' ';
                }
            }
            // 放置 n*k 颗雷
            for _ in 0..N * k {
                self.place_mine(N);
            }
            self.m = (N * N) as i32;
            self.alive = true;

            let mut quit = false;
            {
                let _mode = match MouseMode::enter() {
                    Ok(m) => m,
                    Err(_) => {
                        println!("无法启用鼠标模式，请使用 --legacy 参数运行");
                        return;
                    }
                };
                gamekit::hide_cursor();

                // 游戏主循环（鼠标）
                while self.alive && self.m > (N * k) as i32 {
                    gamekit::clear_screen();
                    print!("{}", self.board_string(false));
                    print!("左键:翻开  右键:标记/取消   q:退出\r\n");

                    match gamekit::read_key_or_mouse() {
                        None => {
                            // 事件读取失败（无 TTY / EOF）：退出
                            quit = true;
                            break;
                        }
                        Some(InputEvent::Key(KeyCode::Char(c))) if c == 'q' || c == 'Q' => {
                            quit = true;
                            break;
                        }
                        Some(InputEvent::Mouse(click)) => {
                            let Some((i, j)) = cell_from_click(click.column, click.row) else {
                                continue;
                            };
                            match click.button {
                                MouseButton::Left => {
                                    // 左键：翻开格子（已翻开或已标记的格子不响应）
                                    if self.b[i][j] == b' ' {
                                        self.m -= self.reveal(i, j);
                                    }
                                }
                                MouseButton::Right => {
                                    // 右键：标记 / 取消标记
                                    if self.b[i][j] == b' ' {
                                        self.b[i][j] = b'P';
                                    } else if self.b[i][j] == b'P' {
                                        self.b[i][j] = b' ';
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }

                // 显示最终棋盘（地雷显示为 *）
                gamekit::clear_screen();
                print!("{}", self.board_string(true));
                if !quit {
                    if self.alive {
                        println!("恭喜！你赢了！点击或按任意键开始下一局");
                    } else {
                        println!("踩到地雷了！点击或按任意键结束");
                    }
                    let _ = gamekit::read_key_or_mouse();
                }
            } // MouseMode 离开作用域：恢复终端

            if quit || !self.alive {
                break 'outer;
            }
            // 赢了则回到难度选择，再来一局
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 鼠标模式：屏幕坐标到棋盘格的映射双向一致，格线处返回 None
    #[test]
    fn click_mapping_round_trip() {
        for i in 1..=N {
            for j in 1..=N {
                // 第 i 行内容在 0 基第 2i 行，第 j 列内容在 0 基第 2+2j 列
                let col = (2 + 2 * j) as u16;
                let row = (2 * i) as u16;
                assert_eq!(cell_from_click(col, row), Some((i, j)), "cell ({i},{j})");
                // 相邻格线 / 偏移位置不映射
                assert_eq!(cell_from_click(col + 1, row), None);
                assert_eq!(cell_from_click(col, row + 1), None);
            }
        }
        // 棋盘外
        assert_eq!(cell_from_click(1, 1), None);
        assert_eq!(cell_from_click(30, 3), None);
    }

    /// 棋盘字符串布局行数正确（标题 + 顶边 + 10 行 + 10 条分隔线）
    #[test]
    fn board_string_layout() {
        let g = Game::new();
        let s = g.board_string(false);
        assert!(s.contains("1 2 3 4 5 6 7 8 9 10"));
        assert_eq!(s.matches("\r\n").count(), 22);
        // 显示雷的版本同样成立
        let s2 = g.board_string(true);
        assert_eq!(s2.matches("\r\n").count(), 22);
    }

    /// q / Q（含首尾空白）视为退出；普通输入不是
    #[test]
    fn quit_detection() {
        assert!(is_quit("q"));
        assert!(is_quit("Q"));
        assert!(is_quit(" q "));
        assert!(!is_quit("1 2p"));
        assert!(!is_quit(""));
    }
}
