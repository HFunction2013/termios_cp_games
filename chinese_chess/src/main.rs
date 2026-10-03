//! 中国象棋 - 增强AI版（避免送棋）—— 原 C++ 版的 Rust 跨平台移植
//!
//! 原程序（GNU C++11，学校版）已自带 Windows/Linux 双分支颜色输出，
//! 本移植统一为 ANSI 转义序列，可在 Windows / macOS / Linux 终端运行。
//!
//! 玩法与原版一致：
//! - 模式：玩家VS玩家 / 玩家VS AI / AI VS AI
//! - 输入格式：行 列 行 列（如 9 4 7 4），特殊命令 99 X
//! - AI 包含 Alpha-Beta 剪枝、启发式移动排序、静态交换评估（SEE/Quiescence）、
//!   迭代深化、时间限制搜索、Zobrist 局面哈希与重复检测

use gamekit;
use gamekit::{InputEvent, KeyCode, MouseButton, MouseMode};
use std::collections::HashMap;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// 枚举定义
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PieceType {
    Empty = 0,
    General,
    Advisor,
    Elephant,
    Horse,
    Chariot,
    Cannon,
    Soldier,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PieceColor {
    None = 0,
    Red,
    Black,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GameState {
    Playing,
    RedWin,
    BlackWin,
    Draw,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AIDifficulty {
    Level1 = 1,
    Level2,
    Level3,
    Level4,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GamePhase {
    Opening,
    Midgame,
    Endgame,
}

/// 循环着法（长将 / 长捉）的定性结果
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PerpetualKind {
    None,
    Check, // 长将
    Chase, // 长捉
}

impl PerpetualKind {
    fn name(self) -> &'static str {
        match self {
            PerpetualKind::None => "无",
            PerpetualKind::Check => "长将",
            PerpetualKind::Chase => "长捉",
        }
    }
}

fn opponent(c: PieceColor) -> PieceColor {
    match c {
        PieceColor::Red => PieceColor::Black,
        _ => PieceColor::Red,
    }
}

fn color_name(c: PieceColor) -> &'static str {
    if c == PieceColor::Red {
        "红方"
    } else {
        "黑方"
    }
}

// ---------------------------------------------------------------------------
// 着法
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
struct Move {
    from_row: i32,
    from_col: i32,
    to_row: i32,
    to_col: i32,
    score: i32,
    safety: i32,
}

impl Move {
    fn new() -> Move {
        Move {
            from_row: 0,
            from_col: 0,
            to_row: 0,
            to_col: 0,
            score: 0,
            safety: 0,
        }
    }

    fn from_pos(fr: i32, fc: i32, tr: i32, tc: i32) -> Move {
        Move {
            from_row: fr,
            from_col: fc,
            to_row: tr,
            to_col: tc,
            score: 0,
            safety: 0,
        }
    }

    fn full(fr: i32, fc: i32, tr: i32, tc: i32, s: i32, sf: i32) -> Move {
        Move {
            from_row: fr,
            from_col: fc,
            to_row: tr,
            to_col: tc,
            score: s,
            safety: sf,
        }
    }

    /// 占位着法（无实际移动）
    fn is_placeholder(&self) -> bool {
        self.from_row == self.to_row && self.from_col == self.to_col
    }
}

// ---------------------------------------------------------------------------
// 棋子
// ---------------------------------------------------------------------------

const PIECE_VALUES: [i32; 8] = [0, 10000, 25, 25, 45, 100, 50, 15];

#[derive(Clone, Copy)]
struct Piece {
    ptype: PieceType,
    color: PieceColor,
}

impl Piece {
    fn new() -> Piece {
        Piece {
            ptype: PieceType::Empty,
            color: PieceColor::None,
        }
    }

    fn with(t: PieceType, c: PieceColor) -> Piece {
        Piece {
            ptype: t,
            color: c,
        }
    }

    fn is_empty(&self) -> bool {
        self.ptype == PieceType::Empty || self.color == PieceColor::None
    }

    /// 棋子字符（不带颜色，用于鼠标模式的落点/选中标记着色）
    fn glyph(&self) -> &'static str {
        if self.is_empty() {
            return "  ";
        }
        if self.color == PieceColor::Red {
            match self.ptype {
                PieceType::General => "帅",
                PieceType::Advisor => "仕",
                PieceType::Elephant => "相",
                PieceType::Horse => "马",
                PieceType::Chariot => "车",
                PieceType::Cannon => "炮",
                PieceType::Soldier => "兵",
                _ => "  ",
            }
        } else {
            match self.ptype {
                PieceType::General => "将",
                PieceType::Advisor => "士",
                PieceType::Elephant => "象",
                PieceType::Horse => "馬",
                PieceType::Chariot => "車",
                PieceType::Cannon => "砲",
                PieceType::Soldier => "卒",
                _ => "  ",
            }
        }
    }

    /// 带颜色的棋子名称
    fn name(&self) -> String {
        if self.is_empty() {
            return "  ".to_string();
        }
        let color_code = if self.color == PieceColor::Red {
            gamekit::color::RED
        } else {
            gamekit::color::GRAY
        };
        format!("{}{}{}", color_code, self.glyph(), gamekit::color::RESET)
    }

    /// 字母表示（保留原版接口）
    #[allow(dead_code)]
    fn letter(&self) -> char {
        if self.color == PieceColor::Red {
            match self.ptype {
                PieceType::General => 'K',
                PieceType::Advisor => 'A',
                PieceType::Elephant => 'E',
                PieceType::Horse => 'H',
                PieceType::Chariot => 'R',
                PieceType::Cannon => 'C',
                PieceType::Soldier => 'P',
                _ => '.',
            }
        } else {
            match self.ptype {
                PieceType::General => 'k',
                PieceType::Advisor => 'a',
                PieceType::Elephant => 'e',
                PieceType::Horse => 'h',
                PieceType::Chariot => 'r',
                PieceType::Cannon => 'c',
                PieceType::Soldier => 'p',
                _ => '.',
            }
        }
    }

    fn value(&self) -> i32 {
        PIECE_VALUES[self.ptype as usize]
    }

    #[allow(dead_code)]
    fn mobility_value(&self) -> i32 {
        match self.ptype {
            PieceType::General => 1,
            PieceType::Advisor => 2,
            PieceType::Elephant => 2,
            PieceType::Horse => 8,
            PieceType::Chariot => 15,
            PieceType::Cannon => 8,
            PieceType::Soldier => 3,
            _ => 0,
        }
    }

    /// Zobrist 棋子索引（红 0~6，黑 7~13）
    fn piece_index(&self) -> usize {
        let color_offset = if self.color == PieceColor::Red {
            0
        } else {
            7
        };
        let type_index = match self.ptype {
            PieceType::General => 0,
            PieceType::Advisor => 1,
            PieceType::Elephant => 2,
            PieceType::Horse => 3,
            PieceType::Chariot => 4,
            PieceType::Cannon => 5,
            PieceType::Soldier => 6,
            _ => 0,
        };
        color_offset + type_index
    }
}

// ---------------------------------------------------------------------------
// Zobrist 哈希表（确定性生成，与具体数值无关，只需前后一致）
// ---------------------------------------------------------------------------

struct SplitMix64(u64);

impl SplitMix64 {
    fn new(seed: u64) -> SplitMix64 {
        SplitMix64(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
}

struct ZobristData {
    table: [[u64; 14]; 90],
    side: u64,
}

static ZOBRIST: std::sync::LazyLock<ZobristData> = std::sync::LazyLock::new(|| {
    let mut rng = SplitMix64::new(123456789);
    let mut table = [[0u64; 14]; 90];
    for i in 0..90 {
        for j in 0..14 {
            table[i][j] = rng.next_u64();
        }
    }
    let side = rng.next_u64();
    ZobristData { table, side }
});

// ---------------------------------------------------------------------------
// 棋盘基础工具（自由函数，便于在不克隆 Board 的前提下做快速试算）
// ---------------------------------------------------------------------------

type Grid = [[Piece; 9]; 10];

const DIRS4: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const DIAG4: [(i32, i32); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
const HORSE_DELTAS: [(i32, i32); 8] = [
    (2, 1),
    (2, -1),
    (-2, 1),
    (-2, -1),
    (1, 2),
    (1, -2),
    (-1, 2),
    (-1, -2),
];
const ELEPHANT_DELTAS: [(i32, i32); 4] = [(2, 2), (2, -2), (-2, 2), (-2, -2)];

fn g_in_board(r: i32, c: i32) -> bool {
    r >= 0 && r < 10 && c >= 0 && c < 9
}

fn g_get(a: &Grid, r: i32, c: i32) -> Piece {
    if g_in_board(r, c) {
        a[r as usize][c as usize]
    } else {
        Piece::new()
    }
}

fn g_set(a: &mut Grid, r: i32, c: i32, p: Piece) {
    if g_in_board(r, c) {
        a[r as usize][c as usize] = p;
    }
}

/// 在九宫格内？
fn in_palace(r: i32, c: i32, color: PieceColor) -> bool {
    if c < 3 || c > 5 {
        return false;
    }
    if color == PieceColor::Red {
        (7..=9).contains(&r)
    } else {
        (0..=2).contains(&r)
    }
}

fn general_legal(a: &Grid, fr: i32, fc: i32, tr: i32, tc: i32, color: PieceColor) -> bool {
    let to = g_get(a, tr, tc);

    // “飞将”：将帅照面，可直接吃掉对方将/帅
    if to.ptype == PieceType::General && to.color != color {
        if fc != tc {
            return false;
        }
        let start = fr.min(tr) + 1;
        let end = fr.max(tr);
        for r in start..end {
            if !g_get(a, r, fc).is_empty() {
                return false;
            }
        }
        return true;
    }

    // 九宫格限制
    if !in_palace(tr, tc, color) {
        return false;
    }
    let dr = (tr - fr).abs();
    let dc = (tc - fc).abs();
    (dr == 1 && dc == 0) || (dr == 0 && dc == 1)
}

fn advisor_legal(a: &Grid, fr: i32, fc: i32, tr: i32, tc: i32, color: PieceColor) -> bool {
    let _ = a;
    let _ = (fr, fc);
    if !in_palace(tr, tc, color) {
        return false;
    }
    let dr = (tr - fr).abs();
    let dc = (tc - fc).abs();
    dr == 1 && dc == 1
}

fn elephant_legal(a: &Grid, fr: i32, fc: i32, tr: i32, tc: i32, color: PieceColor) -> bool {
    // 象/相不能过河
    if color == PieceColor::Red && tr < 5 {
        return false;
    }
    if color == PieceColor::Black && tr > 4 {
        return false;
    }
    let dr = (tr - fr).abs();
    let dc = (tc - fc).abs();
    if dr != 2 || dc != 2 {
        return false;
    }
    // 蹩象眼
    let mr = (fr + tr) / 2;
    let mc = (fc + tc) / 2;
    g_get(a, mr, mc).is_empty()
}

fn horse_legal(a: &Grid, fr: i32, fc: i32, tr: i32, tc: i32) -> bool {
    let dr = (tr - fr).abs();
    let dc = (tc - fc).abs();
    if !((dr == 2 && dc == 1) || (dr == 1 && dc == 2)) {
        return false;
    }
    // 蹩马腿
    if dr == 2 {
        let mr = (fr + tr) / 2;
        if !g_get(a, mr, fc).is_empty() {
            return false;
        }
    } else {
        let mc = (fc + tc) / 2;
        if !g_get(a, fr, mc).is_empty() {
            return false;
        }
    }
    true
}

fn chariot_legal(a: &Grid, fr: i32, fc: i32, tr: i32, tc: i32) -> bool {
    if fr != tr && fc != tc {
        return false;
    }
    if fr == tr {
        let start = fc.min(tc) + 1;
        let end = fc.max(tc);
        for c in start..end {
            if !g_get(a, fr, c).is_empty() {
                return false;
            }
        }
    } else {
        let start = fr.min(tr) + 1;
        let end = fr.max(tr);
        for r in start..end {
            if !g_get(a, r, fc).is_empty() {
                return false;
            }
        }
    }
    true
}

fn cannon_legal(a: &Grid, fr: i32, fc: i32, tr: i32, tc: i32) -> bool {
    if fr != tr && fc != tc {
        return false;
    }
    let to = g_get(a, tr, tc);
    let mut between = 0;
    if fr == tr {
        let start = fc.min(tc) + 1;
        let end = fc.max(tc);
        for c in start..end {
            if !g_get(a, fr, c).is_empty() {
                between += 1;
            }
        }
    } else {
        let start = fr.min(tr) + 1;
        let end = fr.max(tr);
        for r in start..end {
            if !g_get(a, r, fc).is_empty() {
                between += 1;
            }
        }
    }
    if to.is_empty() {
        between == 0
    } else {
        between == 1
    }
}

/// 兵/卒是否已过河
fn soldier_crossed(r: i32, color: PieceColor) -> bool {
    (color == PieceColor::Red && r <= 4) || (color == PieceColor::Black && r >= 5)
}

fn soldier_legal(a: &Grid, fr: i32, fc: i32, tr: i32, tc: i32, color: PieceColor) -> bool {
    let _ = a;
    let dr = tr - fr;
    let dc = (tc - fc).abs();

    if color == PieceColor::Red && dr > 0 {
        return false; // 红兵只能向上（行号减小）
    }
    if color == PieceColor::Black && dr < 0 {
        return false; // 黑卒只能向下（行号增大）
    }

    if soldier_crossed(fr, color) {
        (dr.abs() == 1 && dc == 0) || (dr.abs() == 0 && dc == 1)
    } else {
        dr.abs() == 1 && dc == 0
    }
}

/// 走子是否符合棋子本身的走法（不做“走后是否被将军”的检查）
fn move_legal(a: &Grid, fr: i32, fc: i32, tr: i32, tc: i32) -> bool {
    let from = g_get(a, fr, fc);
    if from.is_empty() {
        return false;
    }
    let to = g_get(a, tr, tc);
    if !to.is_empty() && to.color == from.color {
        return false;
    }
    match from.ptype {
        PieceType::General => general_legal(a, fr, fc, tr, tc, from.color),
        PieceType::Advisor => advisor_legal(a, fr, fc, tr, tc, from.color),
        PieceType::Elephant => elephant_legal(a, fr, fc, tr, tc, from.color),
        PieceType::Horse => horse_legal(a, fr, fc, tr, tc),
        PieceType::Chariot => chariot_legal(a, fr, fc, tr, tc),
        PieceType::Cannon => cannon_legal(a, fr, fc, tr, tc),
        PieceType::Soldier => soldier_legal(a, fr, fc, tr, tc, from.color),
        _ => false,
    }
}

fn find_general(a: &Grid, color: PieceColor) -> Option<(i32, i32)> {
    for r in 0..10i32 {
        for c in 0..9i32 {
            let p = g_get(a, r, c);
            if p.ptype == PieceType::General && p.color == color {
                return Some((r, c));
            }
        }
    }
    None
}

fn has_general(a: &Grid, color: PieceColor) -> bool {
    find_general(a, color).is_some()
}

/// (row, col) 是否被 attacker 方攻击（含“飞将”照面）
fn square_attacked(a: &Grid, row: i32, col: i32, attacker: PieceColor) -> bool {
    // 车 / 炮：沿四个方向扫描，第一个子挡车、第二个子后才能被炮打
    for (dr, dc) in DIRS4 {
        let mut r = row + dr;
        let mut c = col + dc;
        let mut screen = false;
        while g_in_board(r, c) {
            let p = g_get(a, r, c);
            if !p.is_empty() {
                if p.color == attacker {
                    if !screen && p.ptype == PieceType::Chariot {
                        return true;
                    }
                    if screen && p.ptype == PieceType::Cannon {
                        return true;
                    }
                }
                if screen {
                    break; // 第二个子之后不再扫描
                }
                screen = true;
            }
            r += dr;
            c += dc;
        }
    }

    // 马
    for (dr, dc) in HORSE_DELTAS {
        let r = row + dr;
        let c = col + dc;
        if !g_in_board(r, c) {
            continue;
        }
        let p = g_get(a, r, c);
        if p.color == attacker && p.ptype == PieceType::Horse && horse_legal(a, r, c, row, col) {
            return true;
        }
    }

    // 兵 / 卒
    for (dr, dc) in DIRS4 {
        let r = row + dr;
        let c = col + dc;
        if !g_in_board(r, c) {
            continue;
        }
        let p = g_get(a, r, c);
        if p.color == attacker
            && p.ptype == PieceType::Soldier
            && soldier_legal(a, r, c, row, col, attacker)
        {
            return true;
        }
    }

    // 将 / 帅（一步）
    for (dr, dc) in DIRS4 {
        let r = row + dr;
        let c = col + dc;
        if !g_in_board(r, c) {
            continue;
        }
        let p = g_get(a, r, c);
        if p.color == attacker
            && p.ptype == PieceType::General
            && general_legal(a, r, c, row, col, attacker)
        {
            return true;
        }
    }

    // 士 / 仕
    for (dr, dc) in DIAG4 {
        let r = row + dr;
        let c = col + dc;
        if !g_in_board(r, c) {
            continue;
        }
        let p = g_get(a, r, c);
        if p.color == attacker
            && p.ptype == PieceType::Advisor
            && advisor_legal(a, r, c, row, col, attacker)
        {
            return true;
        }
    }

    // 象 / 相
    for (dr, dc) in ELEPHANT_DELTAS {
        let r = row + dr;
        let c = col + dc;
        if !g_in_board(r, c) {
            continue;
        }
        let p = g_get(a, r, c);
        if p.color == attacker
            && p.ptype == PieceType::Elephant
            && elephant_legal(a, r, c, row, col, attacker)
        {
            return true;
        }
    }

    // 飞将：目标格上是对方的将/帅，且攻击方将/帅与其同列、中间无子
    let target = g_get(a, row, col);
    if target.ptype == PieceType::General && target.color != attacker {
        if let Some((ar, ac)) = find_general(a, attacker) {
            if ac == col {
                let start = row.min(ar) + 1;
                let end = row.max(ar);
                let mut clear = true;
                for r in start..end {
                    if !g_get(a, r, col).is_empty() {
                        clear = false;
                        break;
                    }
                }
                if clear {
                    return true;
                }
            }
        }
    }

    false
}

/// (row, col) 是否被 defender 方的其它棋子保护
fn square_protected(a: &Grid, row: i32, col: i32, defender: PieceColor) -> bool {
    for r in 0..10i32 {
        for c in 0..9i32 {
            if r == row && c == col {
                continue;
            }
            let p = g_get(a, r, c);
            if p.is_empty() || p.color != defender {
                continue;
            }
            if move_legal(a, r, c, row, col) {
                return true;
            }
        }
    }
    false
}

fn in_check(a: &Grid, color: PieceColor) -> bool {
    match find_general(a, color) {
        Some((r, c)) => square_attacked(a, r, c, opponent(color)),
        None => false,
    }
}

/// 找出能走到 (row, col) 的、价值最低的 by 方棋子（SEE 用）
fn least_valuable_attacker(a: &Grid, row: i32, col: i32, by: PieceColor) -> Option<(i32, i32)> {
    let mut best: Option<(i32, i32, i32)> = None;
    for r in 0..10i32 {
        for c in 0..9i32 {
            if r == row && c == col {
                continue;
            }
            let p = g_get(a, r, c);
            if p.is_empty() || p.color != by {
                continue;
            }
            if !move_legal(a, r, c, row, col) {
                continue;
            }
            let v = p.value();
            if best.map_or(true, |(bv, _, _)| v < bv) {
                best = Some((v, r, c));
            }
        }
    }
    best.map(|(_, r, c)| (r, c))
}

/// 静态交换评估：side 主动在 (row, col) 上发动吃子序列的最大净收益（可选不吃 → 0）
fn see_capture(a: &mut Grid, row: i32, col: i32, side: PieceColor) -> i32 {
    let (ar, ac) = match least_valuable_attacker(a, row, col, side) {
        Some(x) => x,
        None => return 0,
    };
    let attacker = g_get(a, ar, ac);
    let captured = g_get(a, row, col);
    let cap_val = if captured.is_empty() {
        0
    } else {
        captured.value()
    };

    g_set(a, row, col, attacker);
    g_set(a, ar, ac, Piece::new());

    let gain = cap_val - see_capture(a, row, col, opponent(side));

    g_set(a, ar, ac, attacker);
    g_set(a, row, col, captured);

    if gain > 0 {
        gain
    } else {
        0
    }
}

/// 走完这一步之后，mover 是否构成“捉”。
///
/// `before` + `mv` 唯一确定走完之后的局面，所以只传入「走之前的棋盘快照」即可复算。
/// 这样搜索时对路径上的历史节点也能判定长捉，而无需保存整条棋盘快照链。
///
/// 判定标准（对应象棋规则里的“走子攻击”）：
/// 1. 被攻击的是对方将/帅以外的棋子；
/// 2. 该威胁必须是**这一步新造出来的** —— 由走动的那枚棋子发起，
///    且它在走之前并没有攻击这个目标；
///    （这样可以排除“走之前就已存在的静态威胁”，避免把普通循环着法误判成长捉）
/// 3. 威胁成立：目标无保护（有保护的棋子对方无需应对，不构成真正的“捉”）。
fn is_chasing_move(before: &Grid, mover: PieceColor, mv: &Move) -> bool {
    let moved = g_get(before, mv.from_row, mv.from_col);
    if moved.is_empty() || moved.ptype == PieceType::General {
        return false; // 将/帅本身不构成“捉”
    }
    // 由 before + mv 重建走完之后的棋盘
    let mut after = *before;
    g_set(&mut after, mv.to_row, mv.to_col, moved);
    g_set(&mut after, mv.from_row, mv.from_col, Piece::new());

    let opp = opponent(mover);
    for r in 0..10i32 {
        for c in 0..9i32 {
            let target = g_get(&after, r, c);
            if target.is_empty() || target.color != opp {
                continue;
            }
            if target.ptype == PieceType::General {
                continue;
            }
            // 必须是“走动的那枚棋子”发起的攻击
            if !move_legal(&after, mv.to_row, mv.to_col, r, c) {
                continue;
            }
            // 走之前它就已经在攻击这个目标 -> 不是新威胁
            if move_legal(before, mv.from_row, mv.from_col, r, c) {
                continue;
            }
            if !square_protected(&after, r, c, opp) {
                return true;
            }
        }
    }
    false
}

/// 走某一步之后的静态交换评估（可为负：代表这步会白丢子）
fn see_move(a: &Grid, mv: &Move, color: PieceColor) -> i32 {
    let mut scratch = *a;
    let mover = g_get(&scratch, mv.from_row, mv.from_col);
    let captured = g_get(&scratch, mv.to_row, mv.to_col);
    if mover.is_empty() {
        return 0;
    }
    g_set(&mut scratch, mv.to_row, mv.to_col, mover);
    g_set(&mut scratch, mv.from_row, mv.from_col, Piece::new());
    let gain = if captured.is_empty() {
        0
    } else {
        captured.value()
    };
    gain - see_capture(&mut scratch, mv.to_row, mv.to_col, opponent(color))
}

// ---------------------------------------------------------------------------
// 历史条目（用于长将 / 长捉 / 重复局面判定）
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct HistoryEntry {
    #[allow(dead_code)]
    mv: Move,
    mover: PieceColor,
    #[allow(dead_code)]
    moved: Piece,
    #[allow(dead_code)]
    captured: Piece,
    /// 走完这步后，对方是否被将军
    gave_check: bool,
    /// 走完这步后，是否“捉”（威胁吃掉对方无保护或高价值棋子）
    chasing: bool,
    /// 不可逆着法（吃子或兵卒移动）
    irreversible: bool,
    /// 走完这步后的局面哈希
    #[allow(dead_code)]
    hash: u64,
}

// ---------------------------------------------------------------------------
// 棋盘
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Board {
    board: Grid,
    zobrist_hash: u64,
    position_history: HashMap<u64, u32>,
    fifty_move_rule_counter: i32,
    repetition_count: i32,
    /// 着法历史（含将军/捉子标记），用于循环着法判定
    history: Vec<HistoryEntry>,
    /// 局面哈希序列，hash_history[i] = 走了 i 步之后的哈希
    hash_history: Vec<u64>,
}

/// make/unmake 用的回滚信息（不触碰历史与计数器）
#[derive(Clone, Copy)]
struct Undo {
    captured: Piece,
    hash: u64,
    fifty: i32,
}

impl Board {
    fn new() -> Board {
        let mut b = Board {
            board: [[Piece::new(); 9]; 10],
            zobrist_hash: 0,
            position_history: HashMap::new(),
            fifty_move_rule_counter: 0,
            repetition_count: 0,
            history: Vec::new(),
            hash_history: Vec::new(),
        };
        b.initialize_board();
        b
    }

    fn initialize_board(&mut self) {
        for i in 0..10 {
            for j in 0..9 {
                self.board[i][j] = Piece::new();
            }
        }

        // 黑方（上方）
        let black_back: [PieceType; 9] = [
            PieceType::Chariot,
            PieceType::Horse,
            PieceType::Elephant,
            PieceType::Advisor,
            PieceType::General,
            PieceType::Advisor,
            PieceType::Elephant,
            PieceType::Horse,
            PieceType::Chariot,
        ];
        for (j, &t) in black_back.iter().enumerate() {
            self.board[0][j] = Piece::with(t, PieceColor::Black);
        }
        self.board[2][1] = Piece::with(PieceType::Cannon, PieceColor::Black);
        self.board[2][7] = Piece::with(PieceType::Cannon, PieceColor::Black);
        for j in [0usize, 2, 4, 6, 8] {
            self.board[3][j] = Piece::with(PieceType::Soldier, PieceColor::Black);
        }

        // 红方（下方）
        let red_back: [PieceType; 9] = [
            PieceType::Chariot,
            PieceType::Horse,
            PieceType::Elephant,
            PieceType::Advisor,
            PieceType::General,
            PieceType::Advisor,
            PieceType::Elephant,
            PieceType::Horse,
            PieceType::Chariot,
        ];
        for (j, &t) in red_back.iter().enumerate() {
            self.board[9][j] = Piece::with(t, PieceColor::Red);
        }
        self.board[7][1] = Piece::with(PieceType::Cannon, PieceColor::Red);
        self.board[7][7] = Piece::with(PieceType::Cannon, PieceColor::Red);
        for j in [0usize, 2, 4, 6, 8] {
            self.board[6][j] = Piece::with(PieceType::Soldier, PieceColor::Red);
        }

        self.zobrist_hash = self.compute_zobrist_hash();
        self.reset_position_history();
    }

    fn compute_zobrist_hash(&self) -> u64 {
        let mut hash = 0u64;
        for row in 0..10 {
            for col in 0..9 {
                let piece = self.board[row][col];
                if !piece.is_empty() {
                    hash ^= ZOBRIST.table[row * 9 + col][piece.piece_index()];
                }
            }
        }
        hash
    }

    /// 只更新棋盘与哈希，不动历史/计数器（搜索与着法生成用）
    fn apply_move(&mut self, mv: &Move) -> Undo {
        let from_piece = g_get(&self.board, mv.from_row, mv.from_col);
        let captured = g_get(&self.board, mv.to_row, mv.to_col);
        let undo = Undo {
            captured,
            hash: self.zobrist_hash,
            fifty: self.fifty_move_rule_counter,
        };

        self.zobrist_hash ^= ZOBRIST.table[(mv.from_row * 9 + mv.from_col) as usize]
            [from_piece.piece_index()];
        if !captured.is_empty() {
            self.zobrist_hash ^= ZOBRIST.table[(mv.to_row * 9 + mv.to_col) as usize]
                [captured.piece_index()];
        }
        self.zobrist_hash ^=
            ZOBRIST.table[(mv.to_row * 9 + mv.to_col) as usize][from_piece.piece_index()];
        self.zobrist_hash ^= ZOBRIST.side;

        g_set(&mut self.board, mv.to_row, mv.to_col, from_piece);
        g_set(&mut self.board, mv.from_row, mv.from_col, Piece::new());
        undo
    }

    fn undo_move(&mut self, mv: &Move, undo: &Undo) {
        let from_piece = g_get(&self.board, mv.to_row, mv.to_col);
        g_set(&mut self.board, mv.from_row, mv.from_col, from_piece);
        g_set(&mut self.board, mv.to_row, mv.to_col, undo.captured);
        self.zobrist_hash = undo.hash;
        self.fifty_move_rule_counter = undo.fifty;
    }

    fn record_position(&mut self) {
        self.hash_history.push(self.zobrist_hash);
        let counter = self.position_history.entry(self.zobrist_hash).or_insert(0);
        *counter += 1;
        self.repetition_count = *counter as i32;
    }

    /// 清空重复局面记录（不可逆着法之后调用：此前的局面不可能再重现）
    fn clear_position_history(&mut self) {
        self.position_history.clear();
        self.hash_history.clear();
        self.repetition_count = 0;
    }

    fn is_threefold_repetition(&self) -> bool {
        self.repetition_count >= 3
    }

    /// 50 回合规则：连续 100 个半回合（=50 回合）无吃子且无兵卒移动
    fn is_fifty_move_rule_draw(&self) -> bool {
        self.fifty_move_rule_counter >= 100
    }

    fn reset_position_history(&mut self) {
        self.clear_position_history();
        self.fifty_move_rule_counter = 0;
        self.history.clear();
        self.record_position();
    }

    #[allow(dead_code)]
    fn get_zobrist_hash(&self) -> u64 {
        self.zobrist_hash
    }

    fn get_repetition_count(&self) -> i32 {
        self.repetition_count
    }

    fn get_fifty_move_counter(&self) -> i32 {
        self.fifty_move_rule_counter
    }

    fn get_piece(&self, row: i32, col: i32) -> Piece {
        g_get(&self.board, row, col)
    }

    #[allow(dead_code)]
    fn set_piece(&mut self, row: i32, col: i32, piece: Piece) {
        g_set(&mut self.board, row, col, piece);
    }

    fn is_valid_position(&self, row: i32, col: i32) -> bool {
        g_in_board(row, col)
    }

    fn has_general(&self, color: PieceColor) -> bool {
        has_general(&self.board, color)
    }

    /// 只检查“棋子走法 + 归属”，不检查走后是否被将军
    fn is_pseudo_legal(&self, mv: &Move, color: PieceColor) -> bool {
        if !self.is_valid_position(mv.from_row, mv.from_col)
            || !self.is_valid_position(mv.to_row, mv.to_col)
        {
            return false;
        }
        if mv.from_row == mv.to_row && mv.from_col == mv.to_col {
            return false;
        }
        let from = self.get_piece(mv.from_row, mv.from_col);
        let to = self.get_piece(mv.to_row, mv.to_col);
        if from.is_empty() || from.color != color {
            return false;
        }
        if !to.is_empty() && to.color == color {
            return false;
        }
        self.is_move_legal(mv.from_row, mv.from_col, mv.to_row, mv.to_col)
    }

    /// 执行一步真实着法：完整规则校验 + 维护历史/计数器。
    /// 返回 false 表示非法（棋子走法不对，或走后己方被将军 / 将帅照面）。
    fn make_move(&mut self, mv: &Move, current_player: PieceColor) -> bool {
        if !self.is_pseudo_legal(mv, current_player) {
            return false;
        }
        let before = self.board;
        let undo = self.apply_move(mv);
        // 不能走出“自己送将”的棋：包括走后己方被将军，以及主动造成将帅照面
        if in_check(&self.board, current_player) {
            self.undo_move(mv, &undo);
            return false;
        }

        let opp = opponent(current_player);
        let gave_check = in_check(&self.board, opp);
        let chasing = is_chasing_move(&before, current_player, mv);
        let moved = self.get_piece(mv.to_row, mv.to_col);
        let is_capture = !undo.captured.is_empty();
        let is_soldier = moved.ptype == PieceType::Soldier;
        // 兵/卒横向移动是可以走回去的，因此只有“吃子”和“兵卒向前”才真正不可逆
        let irreversible = is_capture || (is_soldier && mv.to_row != mv.from_row);
        // 50 回合规则：吃子或兵卒移动（含横走）都重置计数
        let resets_fifty = is_capture || is_soldier;

        if irreversible {
            // 不可逆着法之后不可能再重现旧局面，重复记录从头开始
            self.history.clear();
            self.hash_history.clear();
            self.position_history.clear();
            self.repetition_count = 0;
        }
        if resets_fifty {
            self.fifty_move_rule_counter = 0;
        } else {
            self.fifty_move_rule_counter += 1;
        }

        self.history.push(HistoryEntry {
            mv: *mv,
            mover: current_player,
            moved,
            captured: undo.captured,
            gave_check,
            chasing,
            irreversible,
            hash: self.zobrist_hash,
        });
        self.record_position();
        true
    }

    fn is_move_legal(&self, from_row: i32, from_col: i32, to_row: i32, to_col: i32) -> bool {
        move_legal(&self.board, from_row, from_col, to_row, to_col)
    }

    fn is_in_check(&self, color: PieceColor) -> bool {
        in_check(&self.board, color)
    }

    fn is_square_attacked(&self, row: i32, col: i32, attacker: PieceColor) -> bool {
        square_attacked(&self.board, row, col, attacker)
    }

    #[allow(dead_code)]
    fn is_square_protected(&self, row: i32, col: i32, defender: PieceColor) -> bool {
        square_protected(&self.board, row, col, defender)
    }

    // -----------------------------------------------------------------------
    // 着法生成
    // -----------------------------------------------------------------------

    /// 按棋子类型定向生成“伪合法”着法（可能走后自己被将军）
    fn generate_pseudo_legal_moves(&self, color: PieceColor) -> Vec<Move> {
        let mut moves: Vec<Move> = Vec::with_capacity(48);
        let opp = opponent(color);

        for r in 0..10i32 {
            for c in 0..9i32 {
                let p = g_get(&self.board, r, c);
                if p.is_empty() || p.color != color {
                    continue;
                }
                match p.ptype {
                    PieceType::General => {
                        for (dr, dc) in DIRS4 {
                            let tr = r + dr;
                            let tc = c + dc;
                            if g_in_board(tr, tc) && move_legal(&self.board, r, c, tr, tc) {
                                moves.push(Move::from_pos(r, c, tr, tc));
                            }
                        }
                        // 飞将：同列且中间无子时直接吃对方将/帅
                        if let Some((or, oc)) = find_general(&self.board, opp) {
                            if move_legal(&self.board, r, c, or, oc) {
                                moves.push(Move::from_pos(r, c, or, oc));
                            }
                        }
                    }
                    PieceType::Advisor => {
                        for (dr, dc) in DIAG4 {
                            let tr = r + dr;
                            let tc = c + dc;
                            if g_in_board(tr, tc) && move_legal(&self.board, r, c, tr, tc) {
                                moves.push(Move::from_pos(r, c, tr, tc));
                            }
                        }
                    }
                    PieceType::Elephant => {
                        for (dr, dc) in ELEPHANT_DELTAS {
                            let tr = r + dr;
                            let tc = c + dc;
                            if g_in_board(tr, tc) && move_legal(&self.board, r, c, tr, tc) {
                                moves.push(Move::from_pos(r, c, tr, tc));
                            }
                        }
                    }
                    PieceType::Horse => {
                        for (dr, dc) in HORSE_DELTAS {
                            let tr = r + dr;
                            let tc = c + dc;
                            if g_in_board(tr, tc) && move_legal(&self.board, r, c, tr, tc) {
                                moves.push(Move::from_pos(r, c, tr, tc));
                            }
                        }
                    }
                    PieceType::Chariot => {
                        for (dr, dc) in DIRS4 {
                            let mut tr = r + dr;
                            let mut tc = c + dc;
                            while g_in_board(tr, tc) {
                                let tp = g_get(&self.board, tr, tc);
                                if tp.is_empty() {
                                    moves.push(Move::from_pos(r, c, tr, tc));
                                } else {
                                    if tp.color != color {
                                        moves.push(Move::from_pos(r, c, tr, tc));
                                    }
                                    break;
                                }
                                tr += dr;
                                tc += dc;
                            }
                        }
                    }
                    PieceType::Cannon => {
                        for (dr, dc) in DIRS4 {
                            // 无炮架：平移
                            let mut tr = r + dr;
                            let mut tc = c + dc;
                            while g_in_board(tr, tc) && g_get(&self.board, tr, tc).is_empty() {
                                moves.push(Move::from_pos(r, c, tr, tc));
                                tr += dr;
                                tc += dc;
                            }
                            // 越过一个炮架后遇到的第一个敌子可吃
                            let mut er = tr + dr;
                            let mut ec = tc + dc;
                            while g_in_board(er, ec) {
                                let tp = g_get(&self.board, er, ec);
                                if !tp.is_empty() {
                                    if tp.color != color {
                                        moves.push(Move::from_pos(r, c, er, ec));
                                    }
                                    break;
                                }
                                er += dr;
                                ec += dc;
                            }
                        }
                    }
                    PieceType::Soldier => {
                        let forward = if color == PieceColor::Red { -1 } else { 1 };
                        let tr = r + forward;
                        if g_in_board(tr, c) && move_legal(&self.board, r, c, tr, c) {
                            moves.push(Move::from_pos(r, c, tr, c));
                        }
                        if soldier_crossed(r, color) {
                            for dc in [-1i32, 1] {
                                let tc = c + dc;
                                if g_in_board(r, tc) && move_legal(&self.board, r, c, r, tc) {
                                    moves.push(Move::from_pos(r, c, r, tc));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        moves
    }

    /// 完整合法着法（过滤掉走后己方被将军的着法）
    fn get_all_legal_moves(&self, color: PieceColor) -> Vec<Move> {
        let pseudo = self.generate_pseudo_legal_moves(color);
        let mut legal: Vec<Move> = Vec::with_capacity(pseudo.len());
        let mut scratch = self.board;
        for mv in pseudo {
            let from_piece = g_get(&scratch, mv.from_row, mv.from_col);
            let captured = g_get(&scratch, mv.to_row, mv.to_col);
            g_set(&mut scratch, mv.to_row, mv.to_col, from_piece);
            g_set(&mut scratch, mv.from_row, mv.from_col, Piece::new());
            if !in_check(&scratch, color) {
                legal.push(mv);
            }
            g_set(&mut scratch, mv.from_row, mv.from_col, from_piece);
            g_set(&mut scratch, mv.to_row, mv.to_col, captured);
        }
        legal
    }

    // -----------------------------------------------------------------------
    // 终局判定
    // -----------------------------------------------------------------------

    /// 判定当前局面是否终局。
    /// 参数必须是 **待走子的一方**（原版错误地传入刚走完子的一方）。
    fn check_game_state(&self, side_to_move: PieceColor) -> GameState {
        // 1. 将帅存亡
        if !self.has_general(PieceColor::Red) {
            return GameState::BlackWin;
        }
        if !self.has_general(PieceColor::Black) {
            return GameState::RedWin;
        }

        // 2. 将死 / 困毙：无合法着法即判负（中国象棋困毙同样算负）
        if self.get_all_legal_moves(side_to_move).is_empty() {
            return if side_to_move == PieceColor::Red {
                GameState::BlackWin
            } else {
                GameState::RedWin
            };
        }

        // 3. 循环着法：长将 / 长捉 判负，双方均属禁止着法判和，否则三次重复判和
        if self.is_threefold_repetition() {
            let (red_kind, black_kind) = self.analyze_repetition();
            match (red_kind, black_kind) {
                (PerpetualKind::None, PerpetualKind::None) => return GameState::Draw,
                (PerpetualKind::None, _) => return GameState::RedWin,
                (_, PerpetualKind::None) => return GameState::BlackWin,
                (_, _) => return GameState::Draw,
            }
        }

        // 4. 50 回合（100 步）无吃子且无兵卒移动
        if self.is_fifty_move_rule_draw() {
            return GameState::Draw;
        }

        // 5. 双方均无足够进攻子力
        if self.is_insufficient_material() {
            return GameState::Draw;
        }

        GameState::Playing
    }

    /// 回溯“当前局面首次出现”以来的整个循环，给出双方的循环着法定性
    /// 循环着法定性（最终裁决用：要求真正形成三次重复局面）
    fn analyze_repetition(&self) -> (PerpetualKind, PerpetualKind) {
        self.analyze_cycle(true)
    }

    /// `require_three = true`：要求三次重复成立，用于终局裁决。
    /// `require_three = false`：局面只要已重复出现就定性，供 AI 提前预判
    /// “再这么走下去自己会不会被判长将 / 长捉”——让 AI 不必等到循环走完才察觉。
    fn analyze_cycle(&self, require_three: bool) -> (PerpetualKind, PerpetualKind) {
        // 少了“三次重复”这个前提，单步或两步的“循环”会被误判成长将 / 长捉
        if require_three && self.repetition_count < 3 {
            return (PerpetualKind::None, PerpetualKind::None);
        }
        let current = self.zobrist_hash;
        let start = match self.hash_history.iter().position(|&h| h == current) {
            Some(i) => i,
            None => return (PerpetualKind::None, PerpetualKind::None),
        };
        // history[i] 是把 hash_history[i] 变成 hash_history[i+1] 的那一步
        let cycle = &self.history[start..];
        // 循环里不能有不可逆着法，且双方都必须走过子
        if cycle.is_empty() || cycle.iter().any(|e| e.irreversible) {
            return (PerpetualKind::None, PerpetualKind::None);
        }
        // 裁决要求一个完整来回（4 步）；预判放宽到双方各一步即可
        let min_len = if require_three { 4 } else { 2 };
        if cycle.len() < min_len
            || !cycle.iter().any(|e| e.mover == PieceColor::Red)
            || !cycle.iter().any(|e| e.mover == PieceColor::Black)
        {
            return (PerpetualKind::None, PerpetualKind::None);
        }

        let mut red_moves = 0i32;
        let mut red_checks = 0i32;
        let mut red_chases = 0i32;
        let mut black_moves = 0i32;
        let mut black_checks = 0i32;
        let mut black_chases = 0i32;

        for e in cycle {
            if e.mover == PieceColor::Red {
                red_moves += 1;
                if e.gave_check {
                    red_checks += 1;
                }
                if e.chasing {
                    red_chases += 1;
                }
            } else {
                black_moves += 1;
                if e.gave_check {
                    black_checks += 1;
                }
                if e.chasing {
                    black_chases += 1;
                }
            }
        }

        let red_kind = if red_moves > 0 && red_checks == red_moves {
            PerpetualKind::Check
        } else if red_moves > 0 && red_chases == red_moves {
            PerpetualKind::Chase
        } else {
            PerpetualKind::None
        };
        let black_kind = if black_moves > 0 && black_checks == black_moves {
            PerpetualKind::Check
        } else if black_moves > 0 && black_chases == black_moves {
            PerpetualKind::Chase
        } else {
            PerpetualKind::None
        };
        (red_kind, black_kind)
    }

    fn is_insufficient_material(&self) -> bool {
        let mut red_pieces = 0;
        let mut black_pieces = 0;
        let mut red_attack = 0;
        let mut black_attack = 0;

        for r in 0..10 {
            for c in 0..9 {
                let piece = self.board[r][c];
                if piece.is_empty() {
                    continue;
                }
                let attacking = matches!(
                    piece.ptype,
                    PieceType::Chariot
                        | PieceType::Horse
                        | PieceType::Cannon
                        | PieceType::Soldier
                );
                if piece.color == PieceColor::Red {
                    red_pieces += 1;
                    if attacking {
                        red_attack += 1;
                    }
                } else {
                    black_pieces += 1;
                    if attacking {
                        black_attack += 1;
                    }
                }
            }
        }

        // 双方都只剩将/帅
        if red_pieces == 1 && black_pieces == 1 {
            return true;
        }
        // 双方都没有任何进攻子力（只剩仕/相之类）
        red_attack == 0 && black_attack == 0
    }

    /// 终局原因文案。判定顺序必须与 `check_game_state` 完全一致，
    /// 否则会出现“判负原因”和“胜负结果”互相矛盾的情况。
    fn win_reason(&self, side_to_move: PieceColor) -> String {
        // 1. 将帅存亡
        if !self.has_general(PieceColor::Red) {
            return "红帅被吃".to_string();
        }
        if !self.has_general(PieceColor::Black) {
            return "黑将被吃".to_string();
        }
        // 2. 将死 / 困毙（优先于重复局面）
        if self.get_all_legal_moves(side_to_move).is_empty() {
            let name = color_name(side_to_move);
            return if self.is_in_check(side_to_move) {
                format!("{}被将死", name)
            } else {
                format!("{}无着可走（困毙），判负", name)
            };
        }
        // 3. 循环着法：长将 / 长捉
        if self.is_threefold_repetition() {
            return self.perpetual_reason();
        }
        format!("{}获胜", color_name(opponent(side_to_move)))
    }

    /// 三次重复局面下的循环着法裁决文案
    fn perpetual_reason(&self) -> String {
        let (red_kind, black_kind) = self.analyze_repetition();
        match (red_kind, black_kind) {
            (PerpetualKind::None, PerpetualKind::None) => "三次重复局面，判和".to_string(),
            (_, PerpetualKind::None) => format!("红方{}，属禁止着法，判负", red_kind.name()),
            (PerpetualKind::None, _) => format!("黑方{}，属禁止着法，判负", black_kind.name()),
            (_, _) => format!(
                "双方循环着法均属禁止着法（红{} / 黑{}），判和",
                red_kind.name(),
                black_kind.name()
            ),
        }
    }

    fn draw_reason(&self) -> String {
        if self.is_threefold_repetition() {
            return self.perpetual_reason();
        }
        if self.is_fifty_move_rule_draw() {
            return "连续 50 回合（100 步）无吃子且无兵卒移动，判和".to_string();
        }
        if self.is_insufficient_material() {
            return "双方均无足够进攻子力（车马炮兵），判和".to_string();
        }
        "和棋".to_string()
    }

    fn get_game_phase(&self) -> GamePhase {
        let mut piece_count = 0;
        for i in 0..10 {
            for j in 0..9 {
                if !self.board[i][j].is_empty() {
                    piece_count += 1;
                }
            }
        }
        if piece_count > 20 {
            GamePhase::Opening
        } else if piece_count > 10 {
            GamePhase::Midgame
        } else {
            GamePhase::Endgame
        }
    }

    fn display(&self) {
        let y = gamekit::color::YELLOW;
        let reset = gamekit::color::RESET;
        println!("\n{}    0   1   2   3   4   5   6   7   8{}", y, reset);
        println!("{}  ┌───┬───┬───┬───┬───┬───┬───┬───┬───┐{}", y, reset);
        for i in 0..10 {
            if i > 0 {
                println!("{}  ├───┼───┼───┼───┼───┼───┼───┼───┼───┤{}", y, reset);
            }
            print!("{} {}│", y, i);
            for j in 0..9 {
                let piece = self.board[i][j];
                print!("{}", piece.name());
                print!("{} │{}", y, reset);
            }
            println!();
            if i == 4 {
                println!("{}  ├───┼───┼───┼───┼───┼───┼───┼───┼───┤{}", y, reset);
                println!("{}  │    楚      河      汉      界     │{}", y, reset);
            }
        }
        println!("{}  └───┴───┴───┴───┴───┴───┴───┴───┴───┘{}", y, reset);
    }

    // -----------------------------------------------------------------------
    // 评估函数
    // -----------------------------------------------------------------------

    fn evaluate(&self, perspective: PieceColor) -> i32 {
        let mut score = 0;

        for i in 0..10 {
            for j in 0..9 {
                let piece = self.board[i][j];
                if piece.is_empty() {
                    continue;
                }
                let piece_score = PIECE_VALUES[piece.ptype as usize];
                let pos_bonus =
                    self.get_position_bonus(i as i32, j as i32, piece.ptype, piece.color);
                let total = piece_score + pos_bonus;

                if piece.color == perspective {
                    score += total;
                } else {
                    score -= total;
                }
            }
        }

        score += self.evaluate_safety(perspective);

        let opp = opponent(perspective);
        let mobility = self.generate_pseudo_legal_moves(perspective).len() as i32;
        let opp_mobility = self.generate_pseudo_legal_moves(opp).len() as i32;
        score += (mobility - opp_mobility) * 3;

        if self.is_in_check(opp) {
            score += 50;
        }
        if self.is_in_check(perspective) {
            score -= 30;
        }

        let mut center_control = 0;
        for i in 3..=6 {
            for j in 3..=5 {
                if self.is_square_attacked(i, j, perspective) {
                    center_control += 5;
                }
                if self.is_square_attacked(i, j, opp) {
                    center_control -= 5;
                }
            }
        }
        score += center_control;

        score += self.evaluate_coordination(perspective);
        score += self.evaluate_pawn_structure(perspective);
        score
    }

    fn get_position_bonus(&self, row: i32, col: i32, ptype: PieceType, color: PieceColor) -> i32 {
        // 马位置表
        const HORSE_POS: [[i32; 9]; 10] = [
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 5, 8, 10, 8, 10, 8, 5, 0],
            [0, 5, 8, 12, 15, 12, 8, 5, 0],
            [0, 5, 8, 12, 15, 12, 8, 5, 0],
            [0, 0, 5, 8, 10, 8, 5, 0, 0],
            [0, 0, 5, 8, 10, 8, 5, 0, 0],
            [0, 5, 8, 12, 15, 12, 8, 5, 0],
            [0, 5, 8, 10, 8, 10, 8, 5, 0],
            [0, 5, 8, 10, 8, 10, 8, 5, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
        ];
        // 车位置表
        const CHARIOT_POS: [[i32; 9]; 10] = [
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [5, 8, 10, 12, 15, 12, 10, 8, 5],
            [5, 8, 10, 12, 15, 12, 10, 8, 5],
            [5, 8, 10, 12, 15, 12, 10, 8, 5],
            [5, 8, 10, 12, 15, 12, 10, 8, 5],
            [5, 8, 10, 12, 15, 12, 10, 8, 5],
            [5, 8, 10, 12, 15, 12, 10, 8, 5],
            [5, 8, 10, 12, 15, 12, 10, 8, 5],
            [5, 8, 10, 12, 15, 12, 10, 8, 5],
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
        ];
        // 炮位置表
        const CANNON_POS: [[i32; 9]; 10] = [
            [0, 0, 0, 5, 5, 5, 0, 0, 0],
            [0, 5, 8, 10, 15, 10, 8, 5, 0],
            [0, 5, 8, 10, 15, 10, 8, 5, 0],
            [0, 5, 8, 10, 15, 10, 8, 5, 0],
            [0, 0, 5, 8, 10, 8, 5, 0, 0],
            [0, 0, 5, 8, 10, 8, 5, 0, 0],
            [0, 5, 8, 10, 15, 10, 8, 5, 0],
            [0, 5, 8, 10, 15, 10, 8, 5, 0],
            [0, 5, 8, 10, 15, 10, 8, 5, 0],
            [0, 0, 0, 5, 5, 5, 0, 0, 0],
        ];
        // 兵位置表
        const SOLDIER_POS: [[i32; 9]; 10] = [
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [5, 8, 10, 15, 20, 15, 10, 8, 5],
            [10, 15, 20, 25, 30, 25, 20, 15, 10],
            [15, 20, 25, 30, 35, 30, 25, 20, 15],
            [15, 20, 25, 30, 35, 30, 25, 20, 15],
            [10, 15, 20, 25, 30, 25, 20, 15, 10],
            [5, 8, 10, 15, 20, 15, 10, 8, 5],
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
        ];

        let adjusted_row = if color == PieceColor::Red { row } else { 9 - row };
        let mut bonus = 0;

        match ptype {
            PieceType::Horse => bonus = HORSE_POS[adjusted_row as usize][col as usize],
            PieceType::Chariot => bonus = CHARIOT_POS[adjusted_row as usize][col as usize],
            PieceType::Cannon => bonus = CANNON_POS[adjusted_row as usize][col as usize],
            PieceType::Soldier => {
                bonus = SOLDIER_POS[adjusted_row as usize][col as usize];
                if soldier_crossed(row, color) {
                    bonus += 10;
                }
                if (color == PieceColor::Red && row <= 2) || (color == PieceColor::Black && row >= 7)
                {
                    bonus += 15;
                }
            }
            PieceType::General => {
                if col == 4 {
                    bonus += 5;
                }
            }
            _ => {}
        }
        bonus
    }

    fn evaluate_safety(&self, perspective: PieceColor) -> i32 {
        let mut safety_score = 0;
        let opp = opponent(perspective);

        for i in 0..10i32 {
            for j in 0..9i32 {
                let piece = g_get(&self.board, i, j);
                if piece.is_empty() || piece.color != perspective {
                    continue;
                }
                let attacked = square_attacked(&self.board, i, j, opp);
                let protected = square_protected(&self.board, i, j, perspective);

                if attacked {
                    if !protected {
                        safety_score -= piece.value() / 2;
                    } else {
                        safety_score -= piece.value() / 10;
                    }
                } else if protected {
                    safety_score += piece.value() / 20;
                }
            }
        }
        safety_score
    }

    fn evaluate_coordination(&self, perspective: PieceColor) -> i32 {
        let mut coordination = 0;
        let mut chariot_count = 0;
        let mut cannon_count = 0;
        let mut horse_count = 0;

        for i in 0..10 {
            for j in 0..9 {
                let piece = self.board[i][j];
                if piece.color != perspective || piece.is_empty() {
                    continue;
                }
                match piece.ptype {
                    PieceType::Chariot => {
                        chariot_count += 1;
                        if soldier_crossed(i as i32, perspective) {
                            coordination += 15;
                        }
                    }
                    PieceType::Cannon => {
                        cannon_count += 1;
                        if i == 4 || i == 5 {
                            coordination += 10;
                        }
                    }
                    PieceType::Horse => {
                        horse_count += 1;
                        if (3..=6).contains(&i) && (3..=5).contains(&j) {
                            coordination += 10;
                        }
                    }
                    _ => {}
                }
            }
        }

        if chariot_count == 2 {
            coordination += 20;
        }
        if cannon_count == 2 {
            coordination += 10;
        }
        if horse_count == 2 {
            coordination += 10;
        }
        coordination
    }

    fn evaluate_pawn_structure(&self, perspective: PieceColor) -> i32 {
        let mut pawn_score = 0;
        let opp = opponent(perspective);

        let mut pawn_count = 0;
        let mut connected_pawns = 0;
        let mut opp_pawn_count = 0;

        for i in 0..10 {
            for j in 0..9 {
                let piece = self.board[i][j];
                if piece.is_empty() {
                    continue;
                }
                if piece.ptype == PieceType::Soldier && piece.color == perspective {
                    pawn_count += 1;

                    if j > 0 {
                        let left = self.board[i][j - 1];
                        if left.ptype == PieceType::Soldier && left.color == perspective {
                            connected_pawns += 1;
                        }
                    }
                    if j < 8 {
                        let right = self.board[i][j + 1];
                        if right.ptype == PieceType::Soldier && right.color == perspective {
                            connected_pawns += 1;
                        }
                    }

                    if soldier_crossed(i as i32, perspective) {
                        pawn_score += 15;
                    }
                    if (perspective == PieceColor::Red && i <= 2)
                        || (perspective == PieceColor::Black && i >= 7)
                    {
                        pawn_score += 20;
                    }
                } else if piece.ptype == PieceType::Soldier && piece.color == opp {
                    opp_pawn_count += 1;
                }
            }
        }

        pawn_score += (pawn_count - opp_pawn_count) * 10;
        pawn_score += connected_pawns * 5;
        pawn_score
    }

    #[allow(dead_code)]
    fn quick_evaluate(&self, perspective: PieceColor) -> i32 {
        let mut score = 0;
        for i in 0..10 {
            for j in 0..9 {
                let piece = self.board[i][j];
                if piece.is_empty() {
                    continue;
                }
                let value = PIECE_VALUES[piece.ptype as usize];
                if piece.color == perspective {
                    score += value;
                } else {
                    score -= value;
                }
            }
        }
        score
    }
}

// ---------------------------------------------------------------------------
// AI 玩家
// ---------------------------------------------------------------------------

struct ScoredMove {
    mv: Move,
    score: i32,
}

const MATE_SCORE: i32 = 100_000;

/// 搜索路径上的一个节点（用于识别重复局面与循环着法）
struct SearchNode {
    /// 走完这一步之后的局面哈希
    hash: u64,
    /// 走这一步的一方
    mover: PieceColor,
    mv: Move,
    /// 走完这一步是否将军对方（长将判定的依据，几乎零成本）
    gave_check: bool,
    /// 走这一步之前的棋盘快照（配合 mv 即可复算“捉”）
    before: Grid,
}

struct AI {
    color: PieceColor,
    difficulty: AIDifficulty,
    nodes_evaluated: i64,
    max_search_time: Duration,
    start_time: Instant,
    timeout: bool,
    /// 搜索路径上的节点，用于在搜索内部识别重复局面与长将 / 长捉
    search_nodes: Vec<SearchNode>,
}

impl AI {
    fn new(c: PieceColor, d: AIDifficulty) -> AI {
        AI {
            color: c,
            difficulty: d,
            nodes_evaluated: 0,
            max_search_time: Duration::from_millis(1000),
            start_time: Instant::now(),
            timeout: false,
            search_nodes: Vec::new(),
        }
    }

    fn get_best_move(&mut self, board: &mut Board) -> Move {
        self.nodes_evaluated = 0;
        self.timeout = false;
        self.search_nodes.clear();
        self.search_nodes.push(SearchNode {
            hash: board.zobrist_hash,
            mover: self.color,
            mv: Move::new(),
            gave_check: false,
            before: board.board,
        });
        self.start_time = Instant::now();

        // 原版四个难度统一 1000ms
        match self.difficulty {
            AIDifficulty::Level1
            | AIDifficulty::Level2
            | AIDifficulty::Level3
            | AIDifficulty::Level4 => self.max_search_time = Duration::from_millis(1000),
        }

        let legal_moves = board.get_all_legal_moves(self.color);
        if legal_moves.is_empty() {
            return Move::new();
        }

        let mut best_move = match self.difficulty {
            AIDifficulty::Level1 => self.get_smart_heuristic_move(board, &legal_moves),
            // 循环着法（长将 / 长捉）至少要 4 步才成立，搜索深度必须 >= 4
            // AI 才能在搜索里看见并回避；所以三档都用迭代深化把 1 秒用满，
            // 只有最大深度不同（越深越能看到更长的循环）。
            AIDifficulty::Level2 => self.iterative_deepening(board, &legal_moves, 4),
            AIDifficulty::Level3 => self.iterative_deepening(board, &legal_moves, 6),
            AIDifficulty::Level4 => self.iterative_deepening(board, &legal_moves, 8),
        };

        // 未找到好着法时使用启发式
        if best_move.is_placeholder() {
            best_move = self.get_smart_heuristic_move(board, &legal_moves);
        }

        // 能直接将死就直接采用
        {
            let opp = opponent(self.color);
            let mut after = board.clone();
            after.apply_move(&best_move);
            if in_check(&after.board, opp) && after.get_all_legal_moves(opp).is_empty() {
                best_move.score = MATE_SCORE;
                return best_move;
            }
        }

        // 规则闸门：绝不能走出让自己因长将 / 长捉被判负的着法
        if self.would_self_forfeit(board, &best_move) {
            let mut candidates: Vec<ScoredMove> = legal_moves
                .iter()
                .filter(|m| !self.would_self_forfeit(board, m))
                .map(|&m| ScoredMove {
                    mv: m,
                    score: self.evaluate_move_safety(board, &m),
                })
                .collect();
            candidates.sort_by(|a, b| b.score.cmp(&a.score));
            if let Some(c) = candidates.first() {
                best_move = c.mv;
                best_move.score = c.score;
            }
        }

        // 安全检查：若选出的着法会白丢子，换成不吃亏的最佳着法
        if !self.is_move_safe(board, &best_move) {
            let mut candidates: Vec<ScoredMove> = legal_moves
                .iter()
                .filter(|m| self.is_move_safe(board, m) && !self.would_self_forfeit(board, m))
                .map(|&m| ScoredMove {
                    mv: m,
                    score: self.evaluate_move_safety(board, &m),
                })
                .collect();
            candidates.sort_by(|a, b| b.score.cmp(&a.score));
            if let Some(c) = candidates.first() {
                best_move = c.mv;
                best_move.score = c.score;
            }
        }

        best_move
    }

    /// 走这一步会不会让自己因长将 / 长捉被判负。
    /// 用真实棋盘历史模拟一次，只在根节点对候选着法调用。
    ///
    /// 局面只要已重复出现就开始预判，不必等三次重复真正成立——
    /// 这样即便搜索深度不够、看不到完整循环，AI 也不会一步步走进违规。
    fn would_self_forfeit(&self, board: &Board, mv: &Move) -> bool {
        let mut sim = board.clone();
        if !sim.make_move(mv, self.color) {
            return false;
        }
        if sim.get_repetition_count() < 2 {
            return false;
        }
        let (red_kind, black_kind) = sim.analyze_cycle(false);
        match self.color {
            PieceColor::Red => {
                red_kind != PerpetualKind::None && black_kind == PerpetualKind::None
            }
            _ => black_kind != PerpetualKind::None && red_kind == PerpetualKind::None,
        }
    }

    #[allow(dead_code)]
    fn set_difficulty(&mut self, d: AIDifficulty) {
        self.difficulty = d;
    }

    fn get_nodes_evaluated(&self) -> i64 {
        self.nodes_evaluated
    }

    fn check_timeout(&mut self) -> bool {
        if self.timeout {
            return true;
        }
        if self.start_time.elapsed() >= self.max_search_time {
            self.timeout = true;
        }
        self.timeout
    }

    /// 这步棋是否“不吃亏”（静态交换评估 >= 0）
    fn is_move_safe(&self, board: &Board, mv: &Move) -> bool {
        see_move(&board.board, mv, self.color) >= 0
    }

    fn evaluate_move_safety(&self, board: &Board, mv: &Move) -> i32 {
        let mut scratch = board.board;
        let moved_piece = g_get(&scratch, mv.from_row, mv.from_col);
        let target_piece = g_get(&scratch, mv.to_row, mv.to_col);
        if moved_piece.is_empty() {
            return -1000;
        }
        g_set(&mut scratch, mv.to_row, mv.to_col, moved_piece);
        g_set(&mut scratch, mv.from_row, mv.from_col, Piece::new());

        let opp = opponent(self.color);
        let mut safety_score = 100;

        // 1. 移动后是否被将军（含将帅照面）
        if in_check(&scratch, self.color) {
            safety_score -= 500;
        }

        // 2. 移动后棋子是否被攻击
        if square_attacked(&scratch, mv.to_row, mv.to_col, opp) {
            safety_score -= 100;
            if !square_protected(&scratch, mv.to_row, mv.to_col, self.color) {
                safety_score -= 200;
                if moved_piece.value() > 50 {
                    safety_score -= 100;
                }
            }
        }

        // 3. 吃子判断
        if !target_piece.is_empty()
            && (moved_piece.value() as f64) < (target_piece.value() as f64) * 1.5
        {
            safety_score += target_piece.value();
        }

        // 4. 走到安全位置
        if !square_attacked(&scratch, mv.to_row, mv.to_col, opp) {
            safety_score += 50;
        }

        safety_score
    }

    fn get_smart_heuristic_move(&self, board: &Board, moves: &[Move]) -> Move {
        if moves.is_empty() {
            return Move::new();
        }

        let mut scored_moves: Vec<ScoredMove> = Vec::new();
        for &mv in moves {
            let mut score = 0;

            // 吃子奖励
            let target = board.get_piece(mv.to_row, mv.to_col);
            if !target.is_empty() {
                score += target.value() * 3;
            }

            // 移动后是否将军
            let mut scratch = board.board;
            let mover = g_get(&scratch, mv.from_row, mv.from_col);
            g_set(&mut scratch, mv.to_row, mv.to_col, mover);
            g_set(&mut scratch, mv.from_row, mv.from_col, Piece::new());
            if in_check(&scratch, opponent(self.color)) {
                score += 80;
            }

            // 安全性评估
            score += self.evaluate_move_safety(board, &mv);

            // 棋子发展到好位置
            score += self.get_position_score(
                mv.to_row,
                mv.to_col,
                mover.ptype,
                mover.color,
            );

            // 协调性
            score += self.evaluate_move_coordination(board, &mv);

            // 兵/卒前进奖励
            if mover.ptype == PieceType::Soldier {
                if (self.color == PieceColor::Red && mv.to_row < mv.from_row)
                    || (self.color == PieceColor::Black && mv.to_row > mv.from_row)
                {
                    score += 20;
                }
            }

            scored_moves.push(ScoredMove { mv, score });
        }

        scored_moves.sort_by(|a, b| b.score.cmp(&a.score));

        // 优先挑“不吃亏”的高分着法
        for i in 0..scored_moves.len().min(5) {
            if self.is_move_safe(board, &scored_moves[i].mv) {
                return scored_moves[i].mv;
            }
        }
        scored_moves[0].mv
    }

    fn get_position_score(&self, row: i32, col: i32, ptype: PieceType, color: PieceColor) -> i32 {
        if ptype == PieceType::Horse {
            if (3..=6).contains(&row) && (3..=5).contains(&col) {
                return 15;
            }
        } else if ptype == PieceType::Chariot {
            if col == 0 || col == 8 {
                return 10;
            }
            if col == 4 {
                return 15;
            }
        } else if ptype == PieceType::Cannon {
            if row == 4 || row == 5 {
                return 10;
            }
        } else if ptype == PieceType::Soldier && soldier_crossed(row, color) {
            return 15;
        }
        0
    }

    fn evaluate_move_coordination(&self, board: &Board, mv: &Move) -> i32 {
        let mut coordination = 0;
        let moved_piece = board.get_piece(mv.from_row, mv.from_col);

        if matches!(moved_piece.ptype, PieceType::Chariot | PieceType::Cannon) {
            for r in 0..10i32 {
                for c in 0..9i32 {
                    if r == mv.from_row && c == mv.from_col {
                        continue;
                    }
                    let piece = board.get_piece(r, c);
                    if piece.color == self.color
                        && matches!(piece.ptype, PieceType::Chariot | PieceType::Cannon)
                    {
                        if mv.to_row == r || mv.to_col == c {
                            coordination += 10;
                        }
                    }
                }
            }
        } else if moved_piece.ptype == PieceType::Horse {
            for r in 0..10i32 {
                for c in 0..9i32 {
                    if r == mv.from_row && c == mv.from_col {
                        continue;
                    }
                    let piece = board.get_piece(r, c);
                    if piece.color == self.color
                        && matches!(piece.ptype, PieceType::Chariot | PieceType::Cannon)
                    {
                        let distance = (mv.to_row - r).abs() + (mv.to_col - c).abs();
                        if distance <= 3 {
                            coordination += 8;
                        }
                    }
                }
            }
        }
        coordination
    }

    /// 根节点着法排序（吃子优先 + SEE + 安全性）
    fn order_root(&self, board: &Board, moves: &[Move]) -> Vec<ScoredMove> {
        let mut v: Vec<ScoredMove> = Vec::with_capacity(moves.len());
        for &mv in moves {
            let target = board.get_piece(mv.to_row, mv.to_col);
            let mover = board.get_piece(mv.from_row, mv.from_col);
            let mut score = see_move(&board.board, &mv, self.color) * 4;
            if !target.is_empty() {
                score += target.value() * 8 - mover.value();
            }
            score += self.evaluate_move_safety(board, &mv) / 4;
            v.push(ScoredMove { mv, score });
        }
        v.sort_by(|a, b| b.score.cmp(&a.score));
        v
    }

    /// 单轮根搜索（iterative_deepening 会按深度反复调用）
    #[allow(dead_code)]
    fn search_root(&mut self, board: &mut Board, moves: &[Move], depth: i32) -> Move {
        if moves.is_empty() || depth <= 0 {
            return self.get_smart_heuristic_move(board, moves);
        }
        let order = self.order_root(board, moves);
        let mut best_move = order[0].mv;
        let mut best_score = i32::MIN;
        let mut alpha = i32::MIN;

        for sm in &order {
            if self.check_timeout() && best_score > i32::MIN {
                break;
            }
            let before = board.board;
            let undo = board.apply_move(&sm.mv);
            self.search_nodes.push(SearchNode {
                hash: board.zobrist_hash,
                mover: self.color,
                mv: sm.mv,
                gave_check: in_check(&board.board, opponent(self.color)),
                before,
            });
            let score = match self.repetition_value(1) {
                Some(v) => v,
                None => self.alphabeta(board, depth - 1, 1, false, alpha, i32::MAX),
            };
            self.search_nodes.pop();
            board.undo_move(&sm.mv, &undo);

            if score > best_score {
                best_score = score;
                best_move = sm.mv;
                best_move.score = score;
                if score > alpha {
                    alpha = score;
                }
            }
        }
        best_move
    }

    fn iterative_deepening(&mut self, board: &mut Board, moves: &[Move], max_depth: i32) -> Move {
        if moves.is_empty() {
            return Move::new();
        }
        let order = self.order_root(board, moves);
        let mut best_move = order[0].mv;

        for depth in 1..=max_depth {
            let mut cur_best = best_move;
            let mut cur_score = i32::MIN;
            let mut alpha = i32::MIN;
            let mut completed = true;

            for sm in &order {
                if self.check_timeout() {
                    completed = false;
                    break;
                }
                let before = board.board;
                let undo = board.apply_move(&sm.mv);
                self.search_nodes.push(SearchNode {
                    hash: board.zobrist_hash,
                    mover: self.color,
                    mv: sm.mv,
                    gave_check: in_check(&board.board, opponent(self.color)),
                    before,
                });
                let score = match self.repetition_value(1) {
                    Some(v) => v,
                    None => self.alphabeta(board, depth - 1, 1, false, alpha, i32::MAX),
                };
                self.search_nodes.pop();
                board.undo_move(&sm.mv, &undo);

                if score > cur_score {
                    cur_score = score;
                    cur_best = sm.mv;
                    cur_best.score = score;
                    if score > alpha {
                        alpha = score;
                    }
                }
            }

            if cur_score > i32::MIN {
                best_move = cur_best;
            }
            if !completed {
                break;
            }
            if cur_score >= MATE_SCORE - 64 {
                break; // 已找到杀棋，不必再深搜
            }
        }
        best_move
    }

    /// 若当前局面已在搜索路径上出现过，直接给出该局面的估值：
    /// 构成长将 / 长捉的一方判负，双方都违规或都合规则判和（0 分）。
    /// 返回 None 表示未重复，需要继续正常展开。
    ///
    /// 这是让 AI 主动回避违规着法的关键：走长将会在几步之内触发重复
    /// 并拿到判负分，搜索自然会改挑别的着法。
    fn repetition_value(&self, ply: i32) -> Option<i32> {
        // 调用前刚走过的那一步已经入栈，它是循环的最后一环
        let last = self.search_nodes.len().checked_sub(1)?;
        let cur_hash = self.search_nodes[last].hash;
        // 只在「这一步之前」的历史里查找，否则 position 会先匹配到自己
        let start = self.search_nodes[..last].iter().position(|n| n.hash == cur_hash)?;
        // 从「第一次到达该局面之后的那一步」起，到「刚走的这一步」为止，才是真正的循环。
        // 直接用 search_nodes[start..] 会把循环外的那一步也算进来。
        let cycle = &self.search_nodes[start + 1..];
        // 双方各走一步才算一个完整来回（4 步即该局面第 3 次出现）
        if cycle.len() < 4 {
            return Some(0);
        }

        let (mut rm, mut rc, mut rz) = (0i32, 0i32, 0i32);
        let (mut bm, mut bc, mut bz) = (0i32, 0i32, 0i32);
        for n in cycle {
            if n.mover == PieceColor::Red {
                rm += 1;
                if n.gave_check {
                    rc += 1;
                }
                if is_chasing_move(&n.before, PieceColor::Red, &n.mv) {
                    rz += 1;
                }
            } else {
                bm += 1;
                if n.gave_check {
                    bc += 1;
                }
                if is_chasing_move(&n.before, PieceColor::Black, &n.mv) {
                    bz += 1;
                }
            }
        }

        let red_kind = if rm > 0 && rc == rm {
            PerpetualKind::Check
        } else if rm > 0 && rz == rm {
            PerpetualKind::Chase
        } else {
            PerpetualKind::None
        };
        let black_kind = if bm > 0 && bc == bm {
            PerpetualKind::Check
        } else if bm > 0 && bz == bm {
            PerpetualKind::Chase
        } else {
            PerpetualKind::None
        };

        Some(match (red_kind, black_kind) {
            (PerpetualKind::None, PerpetualKind::None) => 0,
            (_, PerpetualKind::None) => {
                if self.color == PieceColor::Red {
                    -(MATE_SCORE - ply)
                } else {
                    MATE_SCORE - ply
                }
            }
            (PerpetualKind::None, _) => {
                if self.color == PieceColor::Black {
                    -(MATE_SCORE - ply)
                } else {
                    MATE_SCORE - ply
                }
            }
            (_, _) => 0,
        })
    }

    fn alphabeta(
        &mut self,
        board: &mut Board,
        depth: i32,
        ply: i32,
        maximizing_player: bool,
        mut alpha: i32,
        mut beta: i32,
    ) -> i32 {
        self.nodes_evaluated += 1;

        if self.check_timeout() {
            return board.evaluate(self.color);
        }

        let stm = if maximizing_player {
            self.color
        } else {
            opponent(self.color)
        };

        // 将帅被吃
        if !board.has_general(stm) {
            return if stm == self.color {
                -(MATE_SCORE - ply)
            } else {
                MATE_SCORE - ply
            };
        }

        let mut d = depth;
        if d <= 0 {
            if in_check(&board.board, stm) {
                d = 1; // 被将军时延伸一层，避免漏算杀棋
            } else {
                return self.quiescence(board, ply, 2, alpha, beta, maximizing_player);
            }
        }

        let moves = board.get_all_legal_moves(stm);
        if moves.is_empty() {
            // 将死或困毙：待走子方判负
            return if stm == self.color {
                -(MATE_SCORE - ply)
            } else {
                MATE_SCORE - ply
            };
        }

        // 移动排序：MVV-LVA（吃子优先，优先用小子吃大子）
        let mut scored_moves: Vec<ScoredMove> = Vec::with_capacity(moves.len());
        for &mv in &moves {
            let mut score = 0;
            let target = board.get_piece(mv.to_row, mv.to_col);
            if !target.is_empty() {
                score += 1000 + target.value() * 8 - board.get_piece(mv.from_row, mv.from_col).value();
            }
            scored_moves.push(ScoredMove { mv, score });
        }
        scored_moves.sort_by(|a, b| b.score.cmp(&a.score));

        if maximizing_player {
            let mut max_eval = i32::MIN;
            for sm in &scored_moves {
                let before = board.board;
                let undo = board.apply_move(&sm.mv);
                self.search_nodes.push(SearchNode {
                    hash: board.zobrist_hash,
                    mover: stm,
                    mv: sm.mv,
                    gave_check: in_check(&board.board, opponent(stm)),
                    before,
                });
                let eval = match self.repetition_value(ply + 1) {
                    Some(v) => v,
                    None => self.alphabeta(board, d - 1, ply + 1, false, alpha, beta),
                };
                self.search_nodes.pop();
                board.undo_move(&sm.mv, &undo);

                if eval > max_eval {
                    max_eval = eval;
                }
                if max_eval > alpha {
                    alpha = max_eval;
                }
                if alpha >= beta {
                    break;
                }
            }
            max_eval
        } else {
            let mut min_eval = i32::MAX;
            for sm in &scored_moves {
                let before = board.board;
                let undo = board.apply_move(&sm.mv);
                self.search_nodes.push(SearchNode {
                    hash: board.zobrist_hash,
                    mover: stm,
                    mv: sm.mv,
                    gave_check: in_check(&board.board, opponent(stm)),
                    before,
                });
                let eval = match self.repetition_value(ply + 1) {
                    Some(v) => v,
                    None => self.alphabeta(board, d - 1, ply + 1, true, alpha, beta),
                };
                self.search_nodes.pop();
                board.undo_move(&sm.mv, &undo);

                if eval < min_eval {
                    min_eval = eval;
                }
                if min_eval < beta {
                    beta = min_eval;
                }
                if alpha >= beta {
                    break;
                }
            }
            min_eval
        }
    }

    fn quiescence(
        &mut self,
        board: &mut Board,
        ply: i32,
        qdepth: i32,
        mut alpha: i32,
        mut beta: i32,
        maximizing_player: bool,
    ) -> i32 {
        self.nodes_evaluated += 1;

        if self.check_timeout() {
            return board.evaluate(self.color);
        }

        let stand_pat = board.evaluate(self.color);
        if qdepth <= 0 {
            return stand_pat;
        }

        let stm = if maximizing_player {
            self.color
        } else {
            opponent(self.color)
        };

        if maximizing_player {
            if stand_pat >= beta {
                return stand_pat;
            }
            if stand_pat > alpha {
                alpha = stand_pat;
            }
        } else {
            if stand_pat <= alpha {
                return stand_pat;
            }
            if stand_pat < beta {
                beta = stand_pat;
            }
        }

        // 只考虑吃子着法，按 MVV-LVA 排序
        let mut captures: Vec<ScoredMove> = Vec::new();
        for mv in board.generate_pseudo_legal_moves(stm) {
            let target = board.get_piece(mv.to_row, mv.to_col);
            if target.is_empty() {
                continue;
            }
            let mover = board.get_piece(mv.from_row, mv.from_col);
            captures.push(ScoredMove {
                mv,
                score: target.value() * 8 - mover.value(),
            });
        }
        captures.sort_by(|a, b| b.score.cmp(&a.score));

        if maximizing_player {
            let mut best = stand_pat;
            for sm in &captures {
                let undo = board.apply_move(&sm.mv);
                let score = if in_check(&board.board, stm) {
                    i32::MIN // 走完自己被将军，这步不算
                } else {
                    self.quiescence(board, ply + 1, qdepth - 1, alpha, beta, false)
                };
                board.undo_move(&sm.mv, &undo);
                if score == i32::MIN {
                    continue;
                }
                if score > best {
                    best = score;
                }
                if best > alpha {
                    alpha = best;
                }
                if alpha >= beta {
                    break;
                }
            }
            best
        } else {
            let mut best = stand_pat;
            for sm in &captures {
                let undo = board.apply_move(&sm.mv);
                let score = if in_check(&board.board, stm) {
                    i32::MAX
                } else {
                    self.quiescence(board, ply + 1, qdepth - 1, alpha, beta, true)
                };
                board.undo_move(&sm.mv, &undo);
                if score == i32::MAX {
                    continue;
                }
                if score < best {
                    best = score;
                }
                if best < beta {
                    beta = best;
                }
                if alpha >= beta {
                    break;
                }
            }
            best
        }
    }
}

// ---------------------------------------------------------------------------
// 游戏流程
// ---------------------------------------------------------------------------

/// 鼠标模式下棋盘的屏幕布局：记录每行棋盘内容所在屏幕行（0 基）、
/// 每个格子内容的起始列与宽度（0 基，含内容后的一个空格，不含格线）。
/// 每个格子固定 4 列：第 j 列格子内容起始列为 3+4j，内容占 2 列。
struct BoardLayout {
    rows: [i32; 10],
    cells: [[(i32, i32); 9]; 10],
}

impl BoardLayout {
    fn new() -> BoardLayout {
        BoardLayout {
            rows: [0; 10],
            cells: [[(0, 0); 9]; 10],
        }
    }

    /// 把终端点击坐标（0 基，与光标位置一致）映射为棋盘 (行, 列)；不在棋盘上返回 None。
    fn cell_at(&self, column: u16, row: u16) -> Option<(i32, i32)> {
        let x = column as i32;
        let y = row as i32;
        for i in 0..10 {
            if self.rows[i] == y {
                for j in 0..9 {
                    let (start, width) = self.cells[i][j];
                    if x >= start && x < start + width + 1 {
                        return Some((i as i32, j as i32));
                    }
                }
            }
        }
        None
    }
}

struct Game {
    board: Board,
    state: GameState,
    current_player: PieceColor,
    red_ai_difficulty: AIDifficulty,
    black_ai_difficulty: AIDifficulty,
    red_is_ai: bool,
    black_is_ai: bool,
    game_log: Vec<String>,
    move_history: Vec<Move>,
    exited: bool,
    end_reason: String,
    /// 鼠标模式下显示在棋盘下方的提示信息（走子结果 / 错误 / AI 动态等）
    status: String,
}

impl Game {
    fn new() -> Game {
        Game {
            board: Board::new(),
            state: GameState::Playing,
            current_player: PieceColor::Red,
            red_ai_difficulty: AIDifficulty::Level3,
            black_ai_difficulty: AIDifficulty::Level3,
            red_is_ai: false,
            black_is_ai: false,
            game_log: Vec::new(),
            move_history: Vec::new(),
            exited: false,
            end_reason: String::new(),
            status: String::new(),
        }
    }

    fn set_game_mode(&mut self, mode: i32, player_color: PieceColor) {
        match mode {
            1 => {
                self.red_is_ai = false;
                self.black_is_ai = false;
            }
            2 => {
                if player_color == PieceColor::Red {
                    self.red_is_ai = false;
                    self.black_is_ai = true;
                } else {
                    self.red_is_ai = true;
                    self.black_is_ai = false;
                }
            }
            3 => {
                self.red_is_ai = true;
                self.black_is_ai = true;
            }
            _ => {}
        }
    }

    fn set_ai_difficulty(&mut self, color: PieceColor, difficulty: AIDifficulty) {
        if color == PieceColor::Red {
            self.red_ai_difficulty = difficulty;
        } else {
            self.black_ai_difficulty = difficulty;
        }
    }

    fn display_game_info(&self) {
        let cyan = gamekit::color::CYAN;
        let red = gamekit::color::RED;
        let yellow = gamekit::color::YELLOW;
        let reset = gamekit::color::RESET;
        println!("{}=== 游戏信息 ==={}", cyan, reset);
        print!("游戏阶段: ");
        match self.board.get_game_phase() {
            GamePhase::Opening => print!("开局"),
            GamePhase::Midgame => print!("中局"),
            GamePhase::Endgame => print!("残局"),
        }

        let repetition_count = self.board.get_repetition_count();
        if repetition_count >= 2 {
            print!(" [重复{}次]", repetition_count);
        }
        let fifty_move_counter = self.board.get_fifty_move_counter();
        if fifty_move_counter >= 50 {
            print!(" [50回合规则: {}/100]", fifty_move_counter);
        }
        if self.board.is_in_check(self.current_player) {
            print!("{} [将军! 必须应将]{}", red, cyan);
        }

        let red_eval = self.board.evaluate(PieceColor::Red);
        println!(
            "\n局面评估: {}",
            if red_eval > 50 {
                "红方优势".to_string()
            } else if red_eval < -50 {
                "黑方优势".to_string()
            } else {
                "均势".to_string()
            }
        );
        println!(" ({})", red_eval);

        // 循环着法预警
        if repetition_count >= 2 {
            let (rk, bk) = self.board.analyze_repetition();
            if rk != PerpetualKind::None || bk != PerpetualKind::None {
                println!(
                    "{}警告：循环着法 红{} / 黑{} —— 禁止着法方将判负！{}",
                    yellow,
                    rk.name(),
                    bk.name(),
                    reset
                );
            }
        }
        println!("{}=== === === ==={}", cyan, reset);
    }

    fn print_rules(&self) {
        let yellow = gamekit::color::YELLOW;
        let reset = gamekit::color::RESET;
        println!("{}\n中国象棋规则（本程序实现）:{}", yellow, reset);
        println!(" 1. 红方先手，黑方后手。");
        println!(" 2. 将/帅只能在九宫格内走一步（上下或左右）。");
        println!(" 3. 士/仕只能在九宫格内斜走一步。");
        println!(" 4. 象/相走田字且不蹩象眼，不能过河。");
        println!(" 5. 马走日，蹩马腿不能走。");
        println!(" 6. 车走直线，中间不能有子。");
        println!(" 7. 炮走直线；吃子必须隔且仅隔一个子（炮架）。");
        println!(" 8. 兵/卒过河前只能向前一步，过河后可左右走一步。");
        println!(" 9. 飞将：将帅同列且中间无子时，可以直接吃掉对方将/帅。");
        println!("10. 不允许走出“送将”的着法：走子后己方被将军（含照面）即为非法。");
        println!("11. 被将军必须应将；无法应将（将死）判负。");
        println!("12. 无着可走（困毙）同样判负。");
        println!("13. 长将（循环中每步都将军）判负。");
        println!("14. 长捉（循环中每步都由走动的子新造出吃子威胁）判负；");
        println!("    双方均为禁止着法则判和。");
        println!("15. 三次重复局面判和。");
        println!("16. 连续 50 回合（100 步）无吃子且无兵卒移动判和。");
        println!("17. 双方均无进攻子力（车马炮兵）判和。");
    }

    /// 开场菜单：标题、规则、模式选择、颜色选择、AI 难度设置（键盘输入）。
    /// 键盘（legacy）与鼠标两种模式共用。
    fn setup(&mut self) {
        let magenta = gamekit::color::MAGENTA;
        let yellow = gamekit::color::YELLOW;
        let reset = gamekit::color::RESET;

        println!("{}========================================={}", magenta, reset);
        println!("{}  中国象棋游戏（增强AI版，避免送棋）{}", magenta, reset);
        println!("{}  Rust 跨平台版 - 完整规则版{}", magenta, reset);
        println!("{}=========================================\n{}", magenta, reset);

        self.print_rules();
        println!("{}=========================================\n{}", yellow, reset);

        // 选择游戏模式
        println!("{}选择游戏模式:{}", yellow, reset);
        println!("1. 玩家 vs 玩家");
        println!("2. 玩家 vs AI");
        println!("3. AI vs AI");
        print!("请选择 (1-3): ");
        use std::io::Write;
        let _ = std::io::stdout().flush();
        let mode = read_int(3).unwrap_or(1);

        let mut player_color = PieceColor::Red;
        if mode == 2 {
            println!("\n{}选择您的颜色:{}", yellow, reset);
            println!("1. 红方（先手）");
            println!("2. 黑方（后手）");
            print!("请选择 (1-2): ");
            let _ = std::io::stdout().flush();
            let color_choice = read_int(2).unwrap_or(1);
            player_color = if color_choice == 1 {
                PieceColor::Red
            } else {
                PieceColor::Black
            };
        }

        self.set_game_mode(mode, player_color);

        if self.red_is_ai || self.black_is_ai {
            println!("\n{}AI难度级别说明:{}", yellow, reset);
            println!("1. 初级 - 简单启发式（不搜索）");
            println!("2. 中级 - 迭代深化至 4 层（约1秒）");
            println!("3. 高级 - 迭代深化至 6 层（约1秒）");
            println!("4. 专家 - 迭代深化至 8 层（约1秒）\n");

            if self.red_is_ai {
                print!("{}选择红方AI难度 (1-4): {}", gamekit::color::RED, reset);
                let _ = std::io::stdout().flush();
                let d = read_int(4).unwrap_or(3);
                self.set_ai_difficulty(PieceColor::Red, difficulty_from(d));
            }
            if self.black_is_ai {
                print!("选择黑方AI难度 (1-4): ");
                let _ = std::io::stdout().flush();
                let d = read_int(4).unwrap_or(3);
                self.set_ai_difficulty(PieceColor::Black, difficulty_from(d));
            }
        }

        println!("{}\n游戏开始！{}", gamekit::color::CYAN, reset);

        if mode == 2 {
            println!(
                "玩家: {}",
                if player_color == PieceColor::Red {
                    "红方"
                } else {
                    "黑方"
                }
            );
            let ai_diff = if player_color == PieceColor::Red {
                self.black_ai_difficulty
            } else {
                self.red_ai_difficulty
            };
            println!(
                "AI: {} (难度: {})",
                if player_color == PieceColor::Red {
                    "黑方"
                } else {
                    "红方"
                },
                ai_diff as i32
            );
        } else {
            println!(
                "红方: {} (难度: {})",
                if self.red_is_ai { "AI" } else { "玩家" },
                self.red_ai_difficulty as i32
            );
            println!(
                "黑方: {} (难度: {})",
                if self.black_is_ai { "AI" } else { "玩家" },
                self.black_ai_difficulty as i32
            );
        }

        println!("\n提示：输入 99 0 查看特殊命令\n");
    }

    fn start(&mut self) {
        let magenta = gamekit::color::MAGENTA;
        let yellow = gamekit::color::YELLOW;
        let reset = gamekit::color::RESET;

        self.setup();

        // 游戏主循环
        while self.state == GameState::Playing && !self.exited {
            self.board.display();
            self.display_game_info();

            print!(
                "{}当前回合: {}{}",
                if self.current_player == PieceColor::Red {
                    gamekit::color::RED
                } else {
                    gamekit::color::GRAY
                },
                color_name(self.current_player),
                reset
            );

            let is_current_player_ai = (self.current_player == PieceColor::Red && self.red_is_ai)
                || (self.current_player == PieceColor::Black && self.black_is_ai);
            if is_current_player_ai {
                println!("{}(AI回合){}", yellow, reset);
            } else {
                println!("{}(玩家回合){}", gamekit::color::GREEN, reset);
            }

            let mv: Move;
            if is_current_player_ai {
                println!("{}AI正在思考...{}", yellow, reset);
                let ai_start = Instant::now();
                let mut ai = AI::new(
                    self.current_player,
                    if self.current_player == PieceColor::Red {
                        self.red_ai_difficulty
                    } else {
                        self.black_ai_difficulty
                    },
                );
                mv = ai.get_best_move(&mut self.board);
                let ai_duration = ai_start.elapsed();

                if mv.is_placeholder() {
                    println!("AI没有找到合法移动！");
                    self.state = if self.current_player == PieceColor::Red {
                        GameState::BlackWin
                    } else {
                        GameState::RedWin
                    };
                    self.end_reason = format!("{}无着可走，判负", color_name(self.current_player));
                    break;
                }

                print!(
                    "{}AI移动: ({},{}) -> ({},{})",
                    gamekit::color::GREEN,
                    mv.from_row,
                    mv.from_col,
                    mv.to_row,
                    mv.to_col
                );
                if mv.score != 0 {
                    print!(" [评分: {}]", mv.score);
                }
                print!(" [思考时间: {}ms]", ai_duration.as_millis());
                print!(" [评估节点: {}]", ai.get_nodes_evaluated());

                let target_piece = self.board.get_piece(mv.to_row, mv.to_col);
                if target_piece.ptype == PieceType::General
                    && target_piece.color != self.current_player
                {
                    print!(" [飞将!]");
                }
                println!("\n{}", reset);
            } else {
                mv = self.get_player_move();
                if mv.from_row == -1 {
                    println!(
                        "{}输入格式错误，请按「行 列 行 列」输入（如 9 4 7 4），可用空格或逗号分隔，99 X 查看特殊命令{}",
                        gamekit::color::RED,
                        reset
                    );
                    continue;
                }
                if mv.from_row == 99 {
                    self.handle_special_command(mv.from_col);
                    continue;
                }
                let moved_piece = self.board.get_piece(mv.from_row, mv.from_col);
                if moved_piece.color != self.current_player {
                    println!("{}错误：不能移动对方的棋子！{}", gamekit::color::RED, reset);
                    continue;
                }
            }

            // 走子前记录源/目标棋子，用于生成日志（避免走子后棋盘已变化导致误判）
            let from_piece = self.board.get_piece(mv.from_row, mv.from_col);
            let to_piece = self.board.get_piece(mv.to_row, mv.to_col);

            if self.board.make_move(&mv, self.current_player) {
                self.move_history.push(mv);

                let mut move_str = format!(
                    "{}: {},{} -> {},{}",
                    color_name(self.current_player),
                    mv.from_row,
                    mv.from_col,
                    mv.to_row,
                    mv.to_col
                );
                if !to_piece.is_empty() {
                    move_str.push_str(&format!(" 吃{}", to_piece.name()));
                } else {
                    move_str.push_str(&format!(" 移动{}", from_piece.name()));
                }
                if to_piece.ptype == PieceType::General && to_piece.color != self.current_player {
                    move_str.push_str(" [飞将!]");
                }
                self.game_log.push(move_str);

                // 交给对方走子，并按“待走子的一方”判定终局
                self.current_player = opponent(self.current_player);
                self.state = self.board.check_game_state(self.current_player);

                if self.state == GameState::Draw {
                    self.end_reason = self.board.draw_reason();
                    println!("{}{}{}", yellow, self.end_reason, reset);
                } else if self.state != GameState::Playing {
                    let target_piece = self.board.get_piece(mv.to_row, mv.to_col);
                    if target_piece.ptype == PieceType::General
                        && target_piece.color != self.current_player
                    {
                        println!("{}\n飞将成功！{}", magenta, reset);
                    }
                    self.end_reason = self.board.win_reason(self.current_player);
                    println!("{}{}{}", magenta, self.end_reason, reset);
                }
            } else if self.board.is_pseudo_legal(&mv, self.current_player) {
                // 走法本身没问题，只是走完之后自己被将军 / 将帅照面
                println!(
                    "{}非法移动：走完之后己方将帅仍被将军（或将帅照面），必须应将！{}",
                    gamekit::color::RED,
                    reset
                );
            } else {
                println!(
                    "{}非法移动：该棋子不能这样走，请重试！{}",
                    gamekit::color::RED,
                    reset
                );
            }
        }

        // 显示游戏结果
        self.board.display();
        println!("{}\n游戏结束！{}", magenta, reset);
        if !self.end_reason.is_empty() {
            println!("结束原因: {}", self.end_reason);
        }
        match self.state {
            GameState::RedWin => println!("红方获胜！"),
            GameState::BlackWin => println!("黑方获胜！"),
            GameState::Draw => println!("和棋！"),
            _ => {}
        }

        println!("\n游戏日志（共{}步）:", self.game_log.len());
        for (i, entry) in self.game_log.iter().enumerate() {
            println!("{}. {}", i + 1, entry);
        }

        println!("{}\n游戏统计:{}", gamekit::color::CYAN, reset);
        println!("总步数: {}", self.game_log.len());
        println!("当前局面重复次数: {}", self.board.get_repetition_count());
        println!(
            "50回合规则计数器: {} / 100 步",
            self.board.get_fifty_move_counter()
        );
    }

    // -----------------------------------------------------------------------
    // 鼠标交互模式
    // -----------------------------------------------------------------------

    /// 渲染一帧鼠标模式画面：棋盘（选中棋子紫色、可落点蓝色标记）+ 回合/提示。
    /// 返回 (画面字符串, 棋盘布局)，布局用于把终端点击坐标映射回棋盘格。
    fn render_mouse(&self, selected: Option<(i32, i32)>) -> (String, BoardLayout) {
        let y = gamekit::color::YELLOW;
        let r = gamekit::color::RESET;
        let magenta = gamekit::color::MAGENTA;
        let blue = gamekit::color::BLUE;
        let green = gamekit::color::GREEN;
        let red = gamekit::color::RED;
        let cyan = gamekit::color::CYAN;
        let gray = gamekit::color::GRAY;
        let yellow = gamekit::color::YELLOW;

        // 收集选中棋子的合法落点（用于蓝色标记）
        let mut dests: Vec<(i32, i32)> = Vec::new();
        if let Some((sr, sc)) = selected {
            if self.board.get_piece(sr, sc).color == self.current_player {
                dests = self
                    .board
                    .get_all_legal_moves(self.current_player)
                    .into_iter()
                    .filter(|m| m.from_row == sr && m.from_col == sc)
                    .map(|m| (m.to_row, m.to_col))
                    .collect();
            }
        }

        let mut layout = BoardLayout::new();
        let mut lines: Vec<String> = Vec::new();
        lines.push(String::new()); // 顶部空行
        lines.push(format!("{}    0   1   2   3   4   5   6   7   8{}", y, r));
        lines.push(format!("{}  ┌───┬───┬───┬───┬───┬───┬───┬───┬───┐{}", y, r));

        for i in 0..10 {
            if i > 0 {
                lines.push(format!(
                    "{}  ├───┼───┼───┼───┼───┼───┼───┼───┼───┤{}",
                    y, r
                ));
            }
            let mut row_line = format!("{} {}│{}", y, i, r);
            // 行号（" 0│"）占 3 个可见字符；每个格子固定占 4 列：
            // 棋子字（CJK 双宽，2 列）或两个空格（2 列）+ " │"（2 列）
            let mut col: i32 = 3;
            for j in 0..9 {
                let piece = self.board.board[i][j];
                let is_selected =
                    selected.map_or(false, |(sr, sc)| sr == i as i32 && sc == j as i32);
                let is_dest = dests
                    .iter()
                    .any(|&(dr, dc)| dr == i as i32 && dc == j as i32);
                let content: String = if piece.is_empty() {
                    if is_dest {
                        // 可落点（空格子）：显示选中棋子的字，蓝色
                        let ghost = selected
                            .and_then(|(sr, sc)| {
                                let p = self.board.get_piece(sr, sc);
                                if p.is_empty() {
                                    None
                                } else {
                                    Some(p.glyph().to_string())
                                }
                            })
                            .unwrap_or_else(|| "  ".to_string());
                        format!("{}{}{}", blue, ghost, r)
                    } else {
                        "  ".to_string()
                    }
                } else if is_selected {
                    // 选中棋子：紫色
                    format!("{}{}{}", magenta, piece.glyph(), r)
                } else if is_dest {
                    // 可吃子：目标敌方棋子变蓝色
                    format!("{}{}{}", blue, piece.glyph(), r)
                } else {
                    piece.name()
                };
                // 内容恒为 2 个终端列宽（棋子为双宽字）
                layout.cells[i][j] = (col, 2);
                row_line.push_str(&content);
                row_line.push_str(&format!(" {}│{}", y, r));
                col += 4;
            }
            lines.push(row_line);
            layout.rows[i] = (lines.len() - 1) as i32;
            if i == 4 {
                lines.push(format!(
                    "{}  ├───┼───┼───┼───┼───┼───┼───┼───┼───┤{}",
                    y, r
                ));
                lines.push(format!("{}  │    楚      河      汉      界     │{}", y, r));
            }
        }
        lines.push(format!("{}  └───┴───┴───┴───┴───┴───┴───┴───┴───┘{}", y, r));

        // 回合 / 将军警告 / 状态 / 操作提示
        let is_ai = (self.current_player == PieceColor::Red && self.red_is_ai)
            || (self.current_player == PieceColor::Black && self.black_is_ai);
        let turn = if self.current_player == PieceColor::Red {
            format!("{}{}回合{}", red, color_name(self.current_player), r)
        } else {
            format!("{}{}回合{}", gray, color_name(self.current_player), r)
        };
        let role = if is_ai {
            format!("{}(AI回合){}", yellow, r)
        } else {
            format!("{}(玩家回合){}", green, r)
        };
        lines.push(format!("{cyan}┈ 当前:{r} {} {}", turn, role));
        if self.board.is_in_check(self.current_player) {
            lines.push(format!("{red}┈ 警告: 将军! 必须应将{r}"));
        }
        if !self.status.is_empty() {
            lines.push(format!("{yellow}┈ {}{r}", self.status));
        }
        lines.push(format!(
            "{cyan}┈ 左键: 选子/落子   右键/Esc: 取消选择   q: 退出   r: 重开{r}"
        ));

        let mut s = lines.join("\r\n");
        s.push_str("\r\n");
        (s, layout)
    }

    /// 检查指定棋子是否有至少一步合法走法（用于将军时限制可选棋子）。
    fn piece_has_legal_move(&self, r: i32, c: i32) -> bool {
        self.board
            .get_all_legal_moves(self.current_player)
            .iter()
            .any(|m| m.from_row == r && m.from_col == c)
    }

    /// 检查指定棋子是否有因“走后己方被将军/飞将”而被过滤掉的伪合法走法。
    /// 用于选中棋子时给出提示（典型场景：过河卒横走会造成将帅照面）。
    fn piece_has_filtered_moves(&self, r: i32, c: i32) -> bool {
        let pseudo_count = self
            .board
            .generate_pseudo_legal_moves(self.current_player)
            .into_iter()
            .filter(|m| m.from_row == r && m.from_col == c)
            .count();
        let legal_count = self
            .board
            .get_all_legal_moves(self.current_player)
            .into_iter()
            .filter(|m| m.from_row == r && m.from_col == c)
            .count();
        pseudo_count > legal_count
    }

    /// 执行走子：落子、记录日志、换手、终局判定，结果写入 status。返回是否成功。
    fn commit_move_mouse(&mut self, mv: Move) -> bool {
        let from_piece = self.board.get_piece(mv.from_row, mv.from_col);
        let to_piece = self.board.get_piece(mv.to_row, mv.to_col);

        if !self.board.make_move(&mv, self.current_player) {
            self.status = "非法移动：该棋子不能这样走".to_string();
            return false;
        }

        self.move_history.push(mv);
        let mut move_str = format!(
            "{}: {},{} -> {},{}",
            color_name(self.current_player),
            mv.from_row,
            mv.from_col,
            mv.to_row,
            mv.to_col
        );
        if !to_piece.is_empty() {
            move_str.push_str(&format!(" 吃{}", to_piece.name()));
        } else {
            move_str.push_str(&format!(" 移动{}", from_piece.name()));
        }
        if to_piece.ptype == PieceType::General && to_piece.color != self.current_player {
            move_str.push_str(" [飞将!]");
        }
        self.game_log.push(move_str.clone());

        self.current_player = opponent(self.current_player);
        self.state = self.board.check_game_state(self.current_player);

        if self.state == GameState::Playing {
            self.status = move_str;
        } else if self.state == GameState::Draw {
            self.end_reason = self.board.draw_reason();
            self.status = format!("{}（和棋：{}）", move_str, self.end_reason);
        } else if self.state != GameState::Playing {
            let target = self.board.get_piece(mv.to_row, mv.to_col);
            if target.ptype == PieceType::General && target.color != self.current_player {
                self.status = format!("{} 飞将成功！", move_str);
            }
            self.end_reason = self.board.win_reason(self.current_player);
            if self.state == GameState::RedWin || self.state == GameState::BlackWin {
                self.status = format!("{}  {}！", move_str, self.end_reason);
            }
        }
        true
    }

    /// 鼠标模式主循环（菜单见 setup，与 legacy 一致）。
    fn start_mouse(&mut self) {
        self.setup();

        // 鼠标模式作用域：结束后先恢复终端，再以正常模式输出结果与日志
        {
        let _mode = match MouseMode::enter() {
            Ok(m) => m,
            Err(_) => {
                println!("无法启用鼠标模式，请使用 --legacy 参数运行");
                return;
            }
        };
        gamekit::hide_cursor();

        let mut selected: Option<(i32, i32)> = None;

        while self.state == GameState::Playing && !self.exited {
            let is_ai = (self.current_player == PieceColor::Red && self.red_is_ai)
                || (self.current_player == PieceColor::Black && self.black_is_ai);

            // AI 回合：渲染“思考中”画面 -> 计算 -> 落子
            if is_ai {
                // 非阻塞检查 q（退出），避免 AI vs AI 时无法中断
                let pending = gamekit::drain_keys();
                if pending.q {
                    self.exited = true;
                    break;
                }
                self.status = format!("{}AI正在思考...", color_name(self.current_player));
                let (board_str, _layout) = self.render_mouse(selected);
                gamekit::clear_screen();
                print!("{}", board_str);
                use std::io::Write;
                let _ = std::io::stdout().flush();

                let ai_start = Instant::now();
                let mut ai = AI::new(
                    self.current_player,
                    if self.current_player == PieceColor::Red {
                        self.red_ai_difficulty
                    } else {
                        self.black_ai_difficulty
                    },
                );
                let mv = ai.get_best_move(&mut self.board);
                let ai_duration = ai_start.elapsed();

                if mv.is_placeholder() {
                    self.status = format!("{}没有找到合法移动！", color_name(self.current_player));
                    self.state = if self.current_player == PieceColor::Red {
                        GameState::BlackWin
                    } else {
                        GameState::RedWin
                    };
                    self.end_reason = format!("{}无着可走，判负", color_name(self.current_player));
                    break;
                }

                let mut info = format!(
                    "{}AI移动: ({},{}) -> ({},{}) [{}ms] [节点{}]",
                    color_name(self.current_player),
                    mv.from_row,
                    mv.from_col,
                    mv.to_row,
                    mv.to_col,
                    ai_duration.as_millis(),
                    ai.get_nodes_evaluated()
                );
                if mv.score != 0 {
                    info.push_str(&format!(" [评分: {}]", mv.score));
                }
                self.commit_move_mouse(mv);
                if !self.status.starts_with("非法") {
                    self.status = format!("{}  {}", info, self.status);
                } else {
                    self.status = info;
                }
                selected = None;
                // 渲染一帧走子结果，让 AI 的着法可见
                let (result_str, _layout) = self.render_mouse(selected);
                gamekit::clear_screen();
                print!("{}", result_str);
                let _ = std::io::stdout().flush();
                gamekit::sleep_ms(350);
                continue;
            }

            // 玩家回合：渲染画面并读取鼠标/按键
            let (board_str, layout) = self.render_mouse(selected);
            gamekit::clear_screen();
            print!("{}", board_str);
            use std::io::Write;
            let _ = std::io::stdout().flush();

            match gamekit::read_key_or_mouse() {
                None => {
                    // 事件读取失败（无 TTY / EOF）：退出游戏
                    self.exited = true;
                }
                Some(InputEvent::Key(KeyCode::Char('q'))) | Some(InputEvent::Key(KeyCode::Char('Q'))) => {
                    self.exited = true;
                }
                Some(InputEvent::Key(KeyCode::Char('r'))) | Some(InputEvent::Key(KeyCode::Char('R'))) => {
                    self.board.initialize_board();
                    self.state = GameState::Playing;
                    self.current_player = PieceColor::Red;
                    self.game_log.clear();
                    self.move_history.clear();
                    self.end_reason.clear();
                    self.status.clear();
                    selected = None;
                }
                Some(InputEvent::Key(KeyCode::Esc)) | Some(InputEvent::Key(KeyCode::Backspace)) => {
                    // 键盘取消选择（部分终端右键被系统菜单截获）
                    selected = None;
                    self.status.clear();
                }
                Some(InputEvent::Mouse(click)) => {
                    if click.button == MouseButton::Right {
                        selected = None;
                        self.status.clear();
                        continue;
                    }
                    if click.button != MouseButton::Left {
                        continue;
                    }
                    let Some((r, c)) = layout.cell_at(click.column, click.row) else {
                        continue;
                    };
                    let piece = self.board.get_piece(r, c);

                    match selected {
                        None => {
                            if piece.color == self.current_player {
                                let in_check = self.board.is_in_check(self.current_player);
                                if in_check && !self.piece_has_legal_move(r, c) {
                                    self.status = format!(
                                        "被将军！{}无法解将，请选择其他可移动的棋子",
                                        piece.name()
                                    );
                                } else {
                                    selected = Some((r, c));
                                    let mut msg = format!(
                                        "已选中 {}（{}），点击蓝色落点走子",
                                        piece.name(),
                                        color_name(self.current_player)
                                    );
                                    if self.piece_has_filtered_moves(r, c) {
                                        msg.push_str("（部分走法因会造成己方被将军/将帅照面而不可用）");
                                    }
                                    self.status = msg;
                                }
                            }
                        }
                        Some((sr, sc)) => {
                            if piece.color == self.current_player {
                                // 点击自己的棋子：切换选择；再点同一枚则取消
                                if sr == r && sc == c {
                                    selected = None;
                                    self.status.clear();
                                } else {
                                    let in_check = self.board.is_in_check(self.current_player);
                                    if in_check && !self.piece_has_legal_move(r, c) {
                                        self.status = format!(
                                            "被将军！{}无法解将，请选择其他可移动的棋子",
                                            piece.name()
                                        );
                                    } else {
                                        selected = Some((r, c));
                                        let mut msg = format!(
                                            "已切换到 {}（{}）",
                                            piece.name(),
                                            color_name(self.current_player)
                                        );
                                        if self.piece_has_filtered_moves(r, c) {
                                            msg.push_str("（部分走法因会造成己方被将军/将帅照面而不可用）");
                                        }
                                        self.status = msg;
                                    }
                                }
                            } else if self.board.get_all_legal_moves(self.current_player).iter().any(
                                |m| {
                                    m.from_row == sr
                                        && m.from_col == sc
                                        && m.to_row == r
                                        && m.to_col == c
                                },
                            ) {
                                // 点击合法落点：走子
                                self.commit_move_mouse(Move::from_pos(sr, sc, r, c));
                                selected = None;
                            } else {
                                // 点到不可达位置：取消选择
                                selected = None;
                                self.status = "该位置不可达，已取消选择".to_string();
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        } // 鼠标模式作用域结束：MouseMode 恢复终端

        // 游戏结束：以正常终端模式显示最终棋盘与结果（与 legacy 一致）
        let magenta = gamekit::color::MAGENTA;
        let reset = gamekit::color::RESET;
        self.board.display();
        println!("{}\n游戏结束！{}", magenta, reset);
        if !self.end_reason.is_empty() {
            println!("结束原因: {}", self.end_reason);
        }
        match self.state {
            GameState::RedWin => println!("红方获胜！"),
            GameState::BlackWin => println!("黑方获胜！"),
            GameState::Draw => println!("和棋！"),
            _ => {}
        }
        println!("\n游戏日志（共{}步）:", self.game_log.len());
        for (i, entry) in self.game_log.iter().enumerate() {
            println!("{}. {}", i + 1, entry);
        }
        println!("{}\n游戏统计:{}", gamekit::color::CYAN, reset);
        println!("总步数: {}", self.game_log.len());
        println!("当前局面重复次数: {}", self.board.get_repetition_count());
        println!(
            "50回合规则计数器: {} / 100 步",
            self.board.get_fifty_move_counter()
        );
    }

    fn get_player_move(&self) -> Move {
        print!("输入移动 (格式: 行 列 行 列) 或输入 99 0 查看特殊命令: ");
        use std::io::Write;
        let _ = std::io::stdout().flush();

        let Some(line) = gamekit::read_line_cooked() else {
            return Move::full(-1, -1, -1, -1, 0, 0);
        };
        // 兼容多种分隔方式：空格、半角/全角逗号；全角数字转半角
        let cleaned: String = line
            .chars()
            .map(|c| match c {
                ',' | '，' => ' ',
                '０'..='９' => char::from_u32('0' as u32 + (c as u32 - '０' as u32)).unwrap_or(c),
                _ => c,
            })
            .collect();
        let nums: Vec<i32> = cleaned
            .split_whitespace()
            .filter_map(|t| t.parse::<i32>().ok())
            .collect();
        // 只有恰好 4 个数字才是走法；99 X 是特殊命令；其余一律视为输入错误
        match nums.as_slice() {
            [99, cmd] => Move::full(99, *cmd, 0, 0, 0, 0),
            [fr, fc, tr, tc] => Move::full(*fr, *fc, *tr, *tc, 0, 0),
            _ => Move::full(-1, -1, -1, -1, 0, 0),
        }
    }

    fn handle_special_command(&mut self, command: i32) {
        let cyan = gamekit::color::CYAN;
        let yellow = gamekit::color::YELLOW;
        let green = gamekit::color::GREEN;
        let reset = gamekit::color::RESET;

        match command {
            0 => {
                println!("{}\n特殊命令:{}", cyan, reset);
                println!("99 0 - 显示帮助");
                println!("99 1 - 显示AI评估");
                println!("99 2 - 显示移动历史");
                println!("99 3 - 重新开始");
                println!("99 4 - 退出游戏");
                println!("99 5 - 切换AI难度");
                println!("99 6 - 显示游戏规则");
                println!("99 7 - 显示局面信息");
                println!("99 8 - 清空局面历史");
                println!("99 9 - 显示AI技术");
            }
            1 => {
                println!("{}当前局面评估:{}", green, reset);
                let red_eval = self.board.evaluate(PieceColor::Red);
                print!("红方视角: {}", red_eval);
                if red_eval > 50 {
                    print!(" (红方优势)");
                } else if red_eval < -50 {
                    print!(" (黑方优势)");
                } else {
                    print!(" (均势)");
                }
                println!();
                let black_eval = self.board.evaluate(PieceColor::Black);
                print!("黑方视角: {}", black_eval);
                if black_eval > 50 {
                    print!(" (黑方优势)");
                } else if black_eval < -50 {
                    print!(" (红方优势)");
                } else {
                    print!(" (均势)");
                }
                println!();

                let mut red_pieces = 0;
                let mut black_pieces = 0;
                for i in 0..10 {
                    for j in 0..9 {
                        let piece = self.board.board[i][j];
                        if piece.is_empty() {
                            continue;
                        }
                        if piece.color == PieceColor::Red {
                            red_pieces += 1;
                        } else {
                            black_pieces += 1;
                        }
                    }
                }
                println!("棋子数量: 红方{}个, 黑方{}个", red_pieces, black_pieces);
            }
            2 => {
                println!("{}游戏日志:{}", yellow, reset);
                for (i, entry) in self.game_log.iter().enumerate() {
                    println!("{}. {}", i + 1, entry);
                }
            }
            3 => {
                self.board.initialize_board();
                self.state = GameState::Playing;
                self.current_player = PieceColor::Red;
                self.game_log.clear();
                self.move_history.clear();
                self.end_reason.clear();
                println!("游戏已重新开始");
            }
            4 => {
                self.exited = true;
                println!("游戏已退出（未分胜负）");
            }
            5 => {
                if self.red_is_ai || self.black_is_ai {
                    println!("{}切换AI难度:{}", yellow, reset);
                    if self.red_is_ai {
                        print!("红方AI当前难度: {}\n", self.red_ai_difficulty as i32);
                        print!("输入新难度 (1-4): ");
                        use std::io::Write;
                        let _ = std::io::stdout().flush();
                        if let Some(difficulty) = read_int(4) {
                            self.red_ai_difficulty = difficulty_from(difficulty);
                            println!("红方AI难度已设置为: {}", self.red_ai_difficulty as i32);
                        } else {
                            println!("无效的难度级别");
                        }
                    }
                    if self.black_is_ai {
                        print!("黑方AI当前难度: {}\n", self.black_ai_difficulty as i32);
                        print!("输入新难度 (1-4): ");
                        use std::io::Write;
                        let _ = std::io::stdout().flush();
                        if let Some(difficulty) = read_int(4) {
                            self.black_ai_difficulty = difficulty_from(difficulty);
                            println!("黑方AI难度已设置为: {}", self.black_ai_difficulty as i32);
                        } else {
                            println!("无效的难度级别");
                        }
                    }
                } else {
                    println!("当前没有AI玩家");
                }
            }
            6 => {
                self.print_rules();
            }
            7 => {
                println!("{}局面信息:{}", cyan, reset);
                println!("当前局面重复次数: {}", self.board.get_repetition_count());
                println!(
                    "50回合规则计数器: {} / 100 步",
                    self.board.get_fifty_move_counter()
                );
                print!("游戏阶段: ");
                match self.board.get_game_phase() {
                    GamePhase::Opening => println!("开局"),
                    GamePhase::Midgame => println!("中局"),
                    GamePhase::Endgame => println!("残局"),
                }
                if self.board.get_repetition_count() >= 2 {
                    let (rk, bk) = self.board.analyze_repetition();
                    println!("循环着法判定: 红{} / 黑{}", rk.name(), bk.name());
                }
                if self.board.is_threefold_repetition() {
                    println!("警告：已达到三次重复局面！");
                }
                if self.board.is_fifty_move_rule_draw() {
                    println!("警告：已达到 50 回合无吃子且无兵卒移动！");
                }
                if self.board.is_insufficient_material() {
                    println!("警告：双方可能无足够进攻子力！");
                }
                if self.board.is_in_check(self.current_player) {
                    println!("提示：{}正被将军，必须应将。", color_name(self.current_player));
                }
            }
            8 => {
                self.board.reset_position_history();
                println!("{}局面历史已清空（重复/50回合计数归零）！{}", green, reset);
            }
            9 => {
                println!("{}AI使用技术:{}", cyan, reset);
                println!("1. Alpha-Beta 剪枝搜索");
                println!("2. 启发式着法排序（MVV-LVA + 静态交换评估 SEE）");
                println!("3. 静态搜索（Quiescence Search）");
                println!("4. 迭代深化 + 时间限制搜索");
                println!("5. 被将军时延伸搜索");
                println!("6. 搜索路径重复局面识别（避免长将/重复）");
                println!("7. 增强评估函数（子力 + 位置 + 安全 + 机动性）");
                println!("8. Zobrist 局面哈希与重复检测");
                println!("9. 完整规则校验：禁止自杀着法、将死/困毙、长将/长捉");
                println!("10. 免送棋：选出着法后做 SEE 复核");
            }
            _ => println!("未知命令"),
        }
    }
}

fn difficulty_from(d: i32) -> AIDifficulty {
    match d {
        1 => AIDifficulty::Level1,
        2 => AIDifficulty::Level2,
        3 => AIDifficulty::Level3,
        _ => AIDifficulty::Level4,
    }
}

/// 读取一个 1..=max 范围内的整数（越界或读取失败返回 None）
fn read_int(max: i32) -> Option<i32> {
    let line = gamekit::read_line_cooked()?;
    let v = line.trim().parse::<i32>().ok()?;
    if (1..=max).contains(&v) {
        Some(v)
    } else {
        None
    }
}

fn main() {
    gamekit::init();
    let legacy = std::env::args().any(|a| a == "--legacy");
    let mut game = Game::new();
    if legacy {
        game.start();
    } else {
        game.start_mouse();
    }

    println!(
        "{}\n感谢游玩中国象棋增强AI版！{}",
        gamekit::color::MAGENTA,
        gamekit::color::RESET
    );
}

// ===========================================================================
// 规则自测：cargo test -p chinese_chess
// 覆盖自杀着法、将帅照面、将死、困毙、长将、长捉、三次重复、50 回合、
// 炮/马/象/兵的走法细节与初始局面着法数。
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn empty_board() -> Board {
        let mut b = Board::new();
        b.board = [[Piece::new(); 9]; 10];
        b.zobrist_hash = 0;
        b.history.clear();
        b.hash_history.clear();
        b.position_history.clear();
        b.fifty_move_rule_counter = 0;
        b.repetition_count = 0;
        b
    }

    fn put(b: &mut Board, r: i32, c: i32, t: PieceType, col: PieceColor) {
        b.set_piece(r, c, Piece::with(t, col));
    }

    fn finish(b: &mut Board) {
        b.zobrist_hash = b.compute_zobrist_hash();
        b.reset_position_history();
    }

    #[test]
    fn initial_position_has_44_legal_moves() {
        let b = Board::new();
        assert_eq!(b.get_all_legal_moves(PieceColor::Red).len(), 44);
        assert_eq!(b.get_all_legal_moves(PieceColor::Black).len(), 44);
    }

    #[test]
    fn suicide_move_is_illegal() {
        let mut b = empty_board();
        put(&mut b, 9, 4, PieceType::General, PieceColor::Red);
        put(&mut b, 6, 0, PieceType::Soldier, PieceColor::Red);
        put(&mut b, 5, 4, PieceType::Chariot, PieceColor::Black);
        put(&mut b, 0, 5, PieceType::General, PieceColor::Black);
        finish(&mut b);
        assert!(b.is_in_check(PieceColor::Red));
        assert!(!b.make_move(&Move::from_pos(6, 0, 5, 0), PieceColor::Red));
        assert!(b.make_move(&Move::from_pos(9, 4, 9, 3), PieceColor::Red));
    }

    #[test]
    fn creating_facing_generals_is_illegal() {
        let mut b = empty_board();
        put(&mut b, 9, 4, PieceType::General, PieceColor::Red);
        put(&mut b, 5, 4, PieceType::Chariot, PieceColor::Red);
        put(&mut b, 0, 4, PieceType::General, PieceColor::Black);
        finish(&mut b);
        assert!(!b.is_in_check(PieceColor::Red));
        assert!(!b.make_move(&Move::from_pos(5, 4, 5, 0), PieceColor::Red));
    }

    #[test]
    fn flying_general_capture() {
        let mut b = empty_board();
        put(&mut b, 9, 4, PieceType::General, PieceColor::Red);
        put(&mut b, 0, 4, PieceType::General, PieceColor::Black);
        finish(&mut b);
        assert!(b.is_in_check(PieceColor::Black));
        assert!(b.is_in_check(PieceColor::Red));
        assert!(b.make_move(&Move::from_pos(9, 4, 0, 4), PieceColor::Red));
        assert_eq!(b.check_game_state(PieceColor::Black), GameState::RedWin);
    }

    #[test]
    fn checkmate_is_detected() {
        // 红帅(9,4) 孤帅，(8,4)(9,3)(9,5) 全被黑车控制 -> 将死
        let mut b = empty_board();
        put(&mut b, 9, 4, PieceType::General, PieceColor::Red);
        put(&mut b, 0, 4, PieceType::Chariot, PieceColor::Black);
        put(&mut b, 9, 0, PieceType::Chariot, PieceColor::Black);
        put(&mut b, 9, 8, PieceType::Chariot, PieceColor::Black);
        put(&mut b, 0, 3, PieceType::General, PieceColor::Black);
        finish(&mut b);
        assert!(b.is_in_check(PieceColor::Red));
        assert!(b.get_all_legal_moves(PieceColor::Red).is_empty());
        assert_eq!(b.check_game_state(PieceColor::Red), GameState::BlackWin);
        assert!(b.win_reason(PieceColor::Red).contains("将死"));
    }

    #[test]
    fn stalemate_is_loss() {
        // 红帅(7,4) 孤立，(8,4)(7,3)(7,5) 均被黑马控制，但 (7,4) 未被攻击 -> 困毙
        let mut b = empty_board();
        put(&mut b, 7, 4, PieceType::General, PieceColor::Red);
        put(&mut b, 6, 3, PieceType::Horse, PieceColor::Black);
        put(&mut b, 5, 2, PieceType::Horse, PieceColor::Black);
        put(&mut b, 0, 3, PieceType::General, PieceColor::Black);
        finish(&mut b);
        assert!(!b.is_in_check(PieceColor::Red));
        assert!(b.get_all_legal_moves(PieceColor::Red).is_empty());
        assert_eq!(b.check_game_state(PieceColor::Red), GameState::BlackWin);
        assert!(b.win_reason(PieceColor::Red).contains("困毙"));
    }

    #[test]
    fn perpetual_check_loses() {
        // 红车(2,3)<->(2,4) 反复照将黑将(0,4)<->(0,3)
        let mut b = empty_board();
        put(&mut b, 9, 5, PieceType::General, PieceColor::Red);
        put(&mut b, 2, 3, PieceType::Chariot, PieceColor::Red);
        put(&mut b, 0, 4, PieceType::General, PieceColor::Black);
        finish(&mut b);

        let cycle = [
            (PieceColor::Red, 2, 3, 2, 4),
            (PieceColor::Black, 0, 4, 0, 3),
            (PieceColor::Red, 2, 4, 2, 3),
            (PieceColor::Black, 0, 3, 0, 4),
        ];
        for _ in 0..2 {
            for &(side, fr, fc, tr, tc) in &cycle {
                assert!(b.make_move(&Move::from_pos(fr, fc, tr, tc), side));
                if side == PieceColor::Red {
                    assert!(b.is_in_check(PieceColor::Black));
                }
            }
        }
        assert_eq!(b.get_repetition_count(), 3);
        let (rk, bk) = b.analyze_repetition();
        assert_eq!(rk, PerpetualKind::Check);
        assert_eq!(bk, PerpetualKind::None);
        assert_eq!(b.check_game_state(PieceColor::Red), GameState::BlackWin);
        assert!(b.win_reason(PieceColor::Red).contains("长将"));
    }

    #[test]
    fn threefold_repetition_without_offence_is_draw() {
        let mut b = empty_board();
        put(&mut b, 9, 4, PieceType::General, PieceColor::Red);
        put(&mut b, 5, 0, PieceType::Chariot, PieceColor::Red);
        put(&mut b, 0, 3, PieceType::General, PieceColor::Black);
        put(&mut b, 4, 8, PieceType::Chariot, PieceColor::Black);
        finish(&mut b);

        let cycle = [
            (PieceColor::Red, 5, 0, 5, 1),
            (PieceColor::Black, 4, 8, 4, 7),
            (PieceColor::Red, 5, 1, 5, 0),
            (PieceColor::Black, 4, 7, 4, 8),
        ];
        for _ in 0..2 {
            for &(side, fr, fc, tr, tc) in &cycle {
                assert!(b.make_move(&Move::from_pos(fr, fc, tr, tc), side));
            }
        }
        assert_eq!(b.get_repetition_count(), 3);
        let (rk, bk) = b.analyze_repetition();
        assert_eq!(rk, PerpetualKind::None);
        assert_eq!(bk, PerpetualKind::None);
        assert_eq!(b.check_game_state(PieceColor::Red), GameState::Draw);
        assert!(b.draw_reason().contains("三次重复"));
    }

    #[test]
    fn perpetual_chase_loses() {
        // 红车在 (0,4)<->(0,5) 反复捉无保护的黑卒 (5,4)<->(5,5)
        let mut b = empty_board();
        put(&mut b, 9, 4, PieceType::General, PieceColor::Red);
        put(&mut b, 0, 4, PieceType::Chariot, PieceColor::Red);
        put(&mut b, 2, 3, PieceType::General, PieceColor::Black);
        put(&mut b, 5, 4, PieceType::Soldier, PieceColor::Black);
        finish(&mut b);

        let cycle = [
            (PieceColor::Black, 5, 4, 5, 5),
            (PieceColor::Red, 0, 4, 0, 5),
            (PieceColor::Black, 5, 5, 5, 4),
            (PieceColor::Red, 0, 5, 0, 4),
        ];
        for _ in 0..2 {
            for &(side, fr, fc, tr, tc) in &cycle {
                assert!(b.make_move(&Move::from_pos(fr, fc, tr, tc), side));
                assert!(!b.is_in_check(PieceColor::Black));
            }
        }
        assert_eq!(b.get_repetition_count(), 3);
        let (rk, bk) = b.analyze_repetition();
        assert_eq!(rk, PerpetualKind::Chase);
        assert_eq!(bk, PerpetualKind::None);
        assert_eq!(b.check_game_state(PieceColor::Black), GameState::BlackWin);
        assert!(b.win_reason(PieceColor::Black).contains("长捉"));
    }

    #[test]
    fn fifty_move_counter_and_reset() {
        let mut b = empty_board();
        put(&mut b, 9, 4, PieceType::General, PieceColor::Red);
        put(&mut b, 5, 0, PieceType::Chariot, PieceColor::Red);
        put(&mut b, 0, 3, PieceType::General, PieceColor::Black);
        put(&mut b, 4, 8, PieceType::Chariot, PieceColor::Black);
        finish(&mut b);

        let cycle = [
            (PieceColor::Red, 5, 0, 5, 1),
            (PieceColor::Black, 4, 8, 4, 7),
            (PieceColor::Red, 5, 1, 5, 0),
            (PieceColor::Black, 4, 7, 4, 8),
        ];
        for i in 0..100 {
            let (side, fr, fc, tr, tc) = cycle[i % 4];
            assert!(b.make_move(&Move::from_pos(fr, fc, tr, tc), side));
        }
        assert_eq!(b.get_fifty_move_counter(), 100);
        assert!(b.is_fifty_move_rule_draw());

        let mut b2 = empty_board();
        put(&mut b2, 9, 4, PieceType::General, PieceColor::Red);
        put(&mut b2, 5, 0, PieceType::Chariot, PieceColor::Red);
        put(&mut b2, 0, 3, PieceType::General, PieceColor::Black);
        put(&mut b2, 5, 6, PieceType::Horse, PieceColor::Black);
        finish(&mut b2);
        assert!(b2.make_move(&Move::from_pos(5, 0, 5, 1), PieceColor::Red));
        assert_eq!(b2.get_fifty_move_counter(), 1);
        assert!(b2.make_move(&Move::from_pos(5, 1, 5, 6), PieceColor::Red));
        assert_eq!(b2.get_fifty_move_counter(), 0);
    }

    #[test]
    fn piece_movement_rules() {
        let mut b = empty_board();
        put(&mut b, 9, 4, PieceType::General, PieceColor::Red);
        put(&mut b, 0, 3, PieceType::General, PieceColor::Black);
        // 炮A(7,0)：无炮架不能吃 (7,3) 的黑马，但可平移到 (7,1)
        put(&mut b, 7, 1, PieceType::Cannon, PieceColor::Red);
        put(&mut b, 7, 3, PieceType::Horse, PieceColor::Black);
        // 炮B(2,0)：隔 (2,3) 己方兵可吃 (2,6) 黑马；不能落在隔着一子的空格 (2,4)
        put(&mut b, 2, 0, PieceType::Cannon, PieceColor::Red);
        put(&mut b, 2, 3, PieceType::Soldier, PieceColor::Red);
        put(&mut b, 2, 6, PieceType::Horse, PieceColor::Black);
        // 马(7,7) 蹩腿
        put(&mut b, 7, 7, PieceType::Horse, PieceColor::Red);
        put(&mut b, 8, 7, PieceType::Soldier, PieceColor::Red);
        // 象(9,2) 塞象眼
        put(&mut b, 9, 2, PieceType::Elephant, PieceColor::Red);
        put(&mut b, 8, 3, PieceType::Soldier, PieceColor::Red);
        // 兵/卒过河与否
        put(&mut b, 4, 4, PieceType::Soldier, PieceColor::Red);
        put(&mut b, 3, 4, PieceType::Soldier, PieceColor::Black);
        finish(&mut b);

        assert!(!b.is_move_legal(7, 1, 7, 3)); // 炮无炮架不能吃
        assert!(b.is_move_legal(7, 1, 7, 0)); // 炮无炮架可平移
        assert!(b.is_move_legal(2, 0, 2, 6)); // 炮隔一子可吃
        assert!(!b.is_move_legal(2, 0, 2, 4)); // 炮不能落在隔子后的空格
        assert!(b.is_move_legal(2, 0, 2, 1)); // 炮可平移到空格
        assert!(!b.is_move_legal(7, 7, 9, 6)); // 蹩马腿
        assert!(b.is_move_legal(7, 7, 5, 6)); // 另一方向无阻 -> 合法
        assert!(!b.is_move_legal(9, 2, 7, 4)); // 塞象眼
        assert!(b.is_move_legal(9, 2, 7, 0)); // 另一方向未塞眼 -> 合法
        assert!(b.is_move_legal(4, 4, 4, 5)); // 过河兵可横走
        assert!(!b.is_move_legal(3, 4, 3, 5)); // 未过河卒不可横走
        assert!(b.is_move_legal(3, 4, 4, 4)); // 未过河卒可直进
    }
    /// 回归：单步“循环”不能被误判成长将。
    ///
    /// 曾经 `analyze_repetition` 缺少“三次重复”的前置条件：吃子后历史被清空，
    /// 只剩最后一步将军时，`red_checks == red_moves == 1` 会被判成“红方长将”，
    /// 于是出现「红方长将，判负」和「红方获胜」同时打印的矛盾。
    #[test]
    fn checkmate_reason_is_not_misreported_as_perpetual() {
        // 黑将(0,3) 被红车照将，(0,4) 会造成将帅照面，(1,3)/(0,2) 被红马控制 -> 将死
        let mut b = empty_board();
        put(&mut b, 9, 4, PieceType::General, PieceColor::Red);
        put(&mut b, 2, 1, PieceType::Horse, PieceColor::Red);
        put(&mut b, 5, 0, PieceType::Chariot, PieceColor::Red);
        put(&mut b, 0, 3, PieceType::General, PieceColor::Black);
        finish(&mut b);

        assert!(b.make_move(&Move::from_pos(5, 0, 5, 3), PieceColor::Red));
        assert_eq!(b.get_repetition_count(), 1); // 根本没有重复
        assert_eq!(b.check_game_state(PieceColor::Black), GameState::RedWin);
        let reason = b.win_reason(PieceColor::Black);
        assert!(reason.contains("将死"), "实际原因: {}", reason);
        assert!(!reason.contains("长将"), "实际原因: {}", reason);
    }

    /// AI 必须能识别“走这步会让自己因长将/长捉被判负”
    #[test]
    fn ai_detects_self_forfeit() {
        // 红车(2,3)<->(2,4) 反复照将黑将(0,4)<->(0,3)，走满两轮后重复计数到 3
        let mut b = empty_board();
        put(&mut b, 9, 5, PieceType::General, PieceColor::Red);
        put(&mut b, 2, 3, PieceType::Chariot, PieceColor::Red);
        put(&mut b, 0, 4, PieceType::General, PieceColor::Black);
        finish(&mut b);

        let cycle = [
            (PieceColor::Red, 2, 3, 2, 4),
            (PieceColor::Black, 0, 4, 0, 3),
            (PieceColor::Red, 2, 4, 2, 3),
            (PieceColor::Black, 0, 3, 0, 4),
        ];
        for _ in 0..2 {
            for &(side, fr, fc, tr, tc) in &cycle {
                assert!(b.make_move(&Move::from_pos(fr, fc, tr, tc), side));
            }
        }
        assert_eq!(b.get_repetition_count(), 3);

        // 红方再照将 -> 自己长将判负，必须被识别出来
        let red_ai = AI::new(PieceColor::Red, AIDifficulty::Level1);
        assert!(red_ai.would_self_forfeit(&b, &Move::from_pos(2, 3, 2, 4)));
        // 黑方只是应将，不算违规
        let black_ai = AI::new(PieceColor::Black, AIDifficulty::Level1);
        assert!(!black_ai.would_self_forfeit(&b, &Move::from_pos(0, 4, 0, 3)));
    }

    /// 端到端：在已经形成长将循环的局面下，AI 最终选出的着法不得让自己违规
    #[test]
    fn ai_avoids_perpetual_check_move() {
        let mut b = empty_board();
        put(&mut b, 9, 5, PieceType::General, PieceColor::Red);
        put(&mut b, 2, 3, PieceType::Chariot, PieceColor::Red);
        put(&mut b, 6, 0, PieceType::Soldier, PieceColor::Red);
        put(&mut b, 0, 4, PieceType::General, PieceColor::Black);
        put(&mut b, 3, 8, PieceType::Chariot, PieceColor::Black);
        finish(&mut b);

        let cycle = [
            (PieceColor::Red, 2, 3, 2, 4),
            (PieceColor::Black, 0, 4, 0, 3),
            (PieceColor::Red, 2, 4, 2, 3),
            (PieceColor::Black, 0, 3, 0, 4),
        ];
        for _ in 0..2 {
            for &(side, fr, fc, tr, tc) in &cycle {
                assert!(b.make_move(&Move::from_pos(fr, fc, tr, tc), side));
            }
        }
        assert_eq!(b.get_repetition_count(), 3);

        let mut ai = AI::new(PieceColor::Red, AIDifficulty::Level3);
        let mv = ai.get_best_move(&mut b);
        assert!(
            !ai.would_self_forfeit(&b, &mv),
            "AI 选出了会让自己因长将被判负的着法: ({},{})->({},{})",
            mv.from_row,
            mv.from_col,
            mv.to_row,
            mv.to_col
        );
    }

    /// 鼠标模式：棋盘布局与点击坐标映射必须双向一致
    #[test]
    fn mouse_layout_round_trip() {
        for selected in [None, Some((9, 4)), Some((0, 4)), Some((4, 7))] {
            let game = Game::new();
            let (board_str, layout) = game.render_mouse(selected);
            assert!(!board_str.is_empty());
            for i in 0..10 {
                for j in 0..9 {
                    // 每格固定 4 列：内容起始列 3+4j、宽 2（棋子为双宽 CJK 字）
                    assert_eq!(
                        layout.cells[i][j],
                        (3 + 4 * j as i32, 2),
                        "selected={selected:?} cell({i},{j}) geometry"
                    );
                    let (start, width) = layout.cells[i][j];
                    // 格子中心列与内容行（均 0 基，与 crossterm 鼠标坐标一致）
                    let col = (start + width / 2) as u16;
                    let row = layout.rows[i] as u16;
                    assert_eq!(
                        layout.cell_at(col, row),
                        Some((i as i32, j as i32)),
                        "selected={selected:?} cell ({i},{j})"
                    );
                }
            }
        }
    }

    /// 鼠标模式：选中棋子呈紫色、可落点呈蓝色；未选中时无紫色
    #[test]
    fn mouse_render_highlights_selection() {
        let game = Game::new();
        let (board_str, _) = game.render_mouse(Some((9, 4))); // 选中红帅
        assert!(board_str.contains("\x1b[95m"), "选中棋子应为紫色");
        assert!(board_str.contains("\x1b[94m"), "可落点应为蓝色");

        let (board_str2, _) = game.render_mouse(None);
        assert!(!board_str2.contains("\x1b[95m"), "未选中时不应出现紫色");
    }

    /// 鼠标模式：过河卒应显示横向可落点
    #[test]
    fn mouse_render_crossed_soldier_shows_horizontal_dests() {
        let mut game = Game::new();
        game.board.board = [[Piece::new(); 9]; 10];
        put(&mut game.board, 9, 4, PieceType::General, PieceColor::Red);
        put(&mut game.board, 4, 4, PieceType::Soldier, PieceColor::Red); // 已过河红兵(行4<=4)
        put(&mut game.board, 0, 3, PieceType::General, PieceColor::Black); // 不同列，避免飞将
        game.board.zobrist_hash = game.board.compute_zobrist_hash();
        game.current_player = PieceColor::Red;

        let (board_str, layout) = game.render_mouse(Some((4, 4)));
        // 过河兵在(4,4)，合法落点应包括：向前(3,4)、向左(4,3)、向右(4,5)
        let blue_count = board_str.matches("\x1b[94m").count();
        assert!(
            blue_count >= 3,
            "过河兵应有至少3个可落点（向前+左右横走），实际蓝色标记数: {}\n{}",
            blue_count,
            board_str
        );

        // 验证坐标映射：点击(4,3)和(4,5)应能映射到正确格子
        assert_eq!(layout.cell_at(3 + 4 * 3 + 1, layout.rows[4] as u16), Some((4, 3)));
        assert_eq!(layout.cell_at(3 + 4 * 5 + 1, layout.rows[4] as u16), Some((4, 5)));
    }

    /// 鼠标模式：未过河卒不应显示横向可落点
    #[test]
    fn mouse_render_uncrossed_soldier_no_horizontal_dests() {
        let mut game = Game::new();
        game.board.board = [[Piece::new(); 9]; 10];
        put(&mut game.board, 9, 4, PieceType::General, PieceColor::Red);
        put(&mut game.board, 6, 4, PieceType::Soldier, PieceColor::Red); // 未过河红兵(行6>4)
        put(&mut game.board, 0, 4, PieceType::General, PieceColor::Black);
        game.board.zobrist_hash = game.board.compute_zobrist_hash();
        game.current_player = PieceColor::Red;

        let (board_str, _) = game.render_mouse(Some((6, 4)));
        // 未过河兵在(6,4)，只有向前(5,4)一个落点
        let blue_count = board_str.matches("\x1b[94m").count();
        assert_eq!(
            blue_count, 1,
            "未过河兵应只有1个可落点（仅向前），实际蓝色标记数: {}\n{}",
            blue_count, board_str
        );
    }

    /// 将军时：只有能解将的棋子才有合法走法，无法解将的棋子应被交互层拒绝选中
    #[test]
    fn in_check_only_escaping_pieces_have_legal_moves() {
        let mut game = Game::new();
        game.board.board = [[Piece::new(); 9]; 10];
        // 红帅(9,4)，黑车(5,4)照将；红兵(6,0)无法解将，红仕(9,3)可挡
        put(&mut game.board, 9, 4, PieceType::General, PieceColor::Red);
        put(&mut game.board, 9, 3, PieceType::Advisor, PieceColor::Red);
        put(&mut game.board, 6, 0, PieceType::Soldier, PieceColor::Red);
        put(&mut game.board, 5, 4, PieceType::Chariot, PieceColor::Black);
        put(&mut game.board, 0, 4, PieceType::General, PieceColor::Black);
        game.board.zobrist_hash = game.board.compute_zobrist_hash();
        game.current_player = PieceColor::Red;

        assert!(game.board.is_in_check(PieceColor::Red), "红方应被将军");
        // 红兵(6,0)无法解将
        assert!(
            !game.piece_has_legal_move(6, 0),
            "红兵(6,0)无法解将，不应有合法走法"
        );
        // 红仕(9,3)可以走到(8,4)挡将
        assert!(
            game.piece_has_legal_move(9, 3),
            "红仕(9,3)应能解将（走到8,4挡将）"
        );
        // 红帅(9,4)可以左右移动躲将
        assert!(
            game.piece_has_legal_move(9, 4),
            "红帅(9,4)应能躲将"
        );
    }

    /// 过河卒横走会造成将帅照面时，该横走应被过滤，且 piece_has_filtered_moves 应返回 true
    #[test]
    fn crossed_soldier_horizontal_move_filtered_by_flying_general() {
        let mut game = Game::new();
        game.board.board = [[Piece::new(); 9]; 10];
        // 红帅(9,4)，黑将(0,4)同列；红兵(4,4)在中间遮挡，横走会造成将帅照面
        put(&mut game.board, 9, 4, PieceType::General, PieceColor::Red);
        put(&mut game.board, 4, 4, PieceType::Soldier, PieceColor::Red);
        put(&mut game.board, 0, 4, PieceType::General, PieceColor::Black);
        game.board.zobrist_hash = game.board.compute_zobrist_hash();
        game.current_player = PieceColor::Red;

        // 红兵(4,4)已过河，伪合法走法包括向前+左右横走，但横走会造成将帅照面被过滤
        assert!(
            game.piece_has_filtered_moves(4, 4),
            "红兵(4,4)横走会造成将帅照面，应有被过滤的走法"
        );
        // 只有向前(3,4)是合法的（仍保持遮挡）
        let legal: Vec<(i32, i32)> = game
            .board
            .get_all_legal_moves(PieceColor::Red)
            .into_iter()
            .filter(|m| m.from_row == 4 && m.from_col == 4)
            .map(|m| (m.to_row, m.to_col))
            .collect();
        assert_eq!(legal, vec![(3, 4)], "红兵(4,4)应只能向前走以保持遮挡");
    }
}
