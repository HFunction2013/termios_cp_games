//! 扫雷 —— 原 C++（Windows 版）的 Rust 跨平台移植
//!
//! 玩法与原版完全一致：
//! - 10×10 棋盘，难度 1~5（雷数 = 10 × 难度）
//! - 输入“行 列”翻开格子，行/列后加 p 表示标记地雷，如：1 2p
//! - 全部非雷格子翻开即胜利
//!
//! 已用 ANSI 转义序列 + 标准输入替代 system("cls") / windows.h，
//! 可在 Windows / macOS / Linux 终端中运行。

use gamekit;
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

    /// 打印棋盘（含标题栏）
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
}

fn main() {
    gamekit::init();
    let mut game = Game::new();
    let mut k: usize;

    println!("欢迎来到扫雷游戏");

    while game.alive {
        // 选择难度
        loop {
            print!("请选择难度(1~5):");
            use std::io::Write;
            let _ = std::io::stdout().flush();
            match gamekit::read_line_cooked() {
                Some(line) => match line.trim().parse::<i32>() {
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
                },
                None => return, // EOF 退出
            }
        }

        // 初始化棋盘
        for i in 1..=N {
            for j in 1..=N {
                game.a[i][j] = b'0';
                game.b[i][j] = b' ';
            }
        }
        // 放置 n*k 颗雷
        for _ in 0..N * k {
            game.place_mine(N);
        }
        game.m = (N * N) as i32;
        game.alive = true;

        // 游戏主循环
        while game.alive && game.m > (N * k) as i32 {
            gamekit::clear_screen();
            game.print_board(false);

            print!("请输入坐标(在坐标后加上p表示标记,如 1 2p):");
            use std::io::Write;
            let _ = std::io::stdout().flush();

            let Some(line) = gamekit::read_line_cooked() else { return };
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
                game.m -= game.reveal(x, y);
            } else {
                if game.b[x][y] == b' ' {
                    game.b[x][y] = b'P';
                } else {
                    println!("无法标记");
                    gamekit::sleep_ms(1000);
                }
            }
        }

        // 显示最终棋盘（地雷显示为 *）
        gamekit::clear_screen();
        game.print_board(true);
    }

    print!("按回车键结束");
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let _ = gamekit::read_line_cooked();
}
