//! 中国象棋 - 增强AI版（避免送棋）—— 原 C++ 版的 Rust 跨平台移植
//!
//! 原程序（GNU C++11，学校版）已自带 Windows/Linux 双分支颜色输出，
//! 本移植统一为 ANSI 转义序列，可在 Windows / macOS / Linux 终端运行。
//!
//! 玩法与原版一致：
//! - 模式：玩家VS玩家 / 玩家VS AI / AI VS AI
//! - 输入格式：行 列 行 列（如 9 4 7 4），特殊命令 99 X
//! - AI 包含 Alpha-Beta 剪枝、启发式移动排序、静态交换评估（Quiescence）、
//!   迭代深化、时间限制搜索、Zobrist 局面哈希与重复检测
//!
//! 与原版的一处行为差异：原版用 std::set 统计重复局面（恒为 1，三次重复
//! 判和永远不触发），本移植改为用计数器统计，使“三次重复局面判和”规则
//! 真正生效，与代码注释的设计意图一致。

use gamekit;
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

    /// 带颜色的棋子名称
    fn name(&self) -> String {
        if self.is_empty() {
            return "  ".to_string();
        }
        let name = if self.color == PieceColor::Red {
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
        };
        let color_code = if self.color == PieceColor::Red {
            gamekit::color::RED
        } else {
            gamekit::color::GRAY
        };
        format!("{}{}{}", color_code, name, gamekit::color::RESET)
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
        let color_offset = if self.color == PieceColor::Red { 0 } else { 7 };
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
// 棋盘
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Board {
    board: [[Piece; 9]; 10],
    zobrist_hash: u64,
    position_history: HashMap<u64, u32>,
    fifty_move_rule_counter: i32,
    repetition_count: i32,
}

impl Board {
    fn new() -> Board {
        let mut b = Board {
            board: [[Piece::new(); 9]; 10],
            zobrist_hash: 0,
            position_history: HashMap::new(),
            fifty_move_rule_counter: 0,
            repetition_count: 0,
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
        self.record_position();
    }

    fn compute_zobrist_hash(&self) -> u64 {
        let mut hash = 0u64;
        for row in 0..10 {
            for col in 0..9 {
                let piece = self.board[row][col];
                if piece.ptype != PieceType::Empty && piece.color != PieceColor::None {
                    hash ^= ZOBRIST.table[row * 9 + col][piece.piece_index()];
                }
            }
        }
        hash
    }

    fn update_zobrist_hash(&mut self, from_row: i32, from_col: i32, to_row: i32, to_col: i32, captured: Piece) {
        let from_piece = self.get_piece(from_row, from_col);
        let from_index = from_row as usize * 9 + from_col as usize;
        self.zobrist_hash ^= ZOBRIST.table[from_index][from_piece.piece_index()];

        if captured.ptype != PieceType::Empty {
            let captured_index = to_row as usize * 9 + to_col as usize;
            self.zobrist_hash ^= ZOBRIST.table[captured_index][captured.piece_index()];
        }

        let to_index = to_row as usize * 9 + to_col as usize;
        self.zobrist_hash ^= ZOBRIST.table[to_index][from_piece.piece_index()];
        self.zobrist_hash ^= ZOBRIST.side;
    }

    fn record_position(&mut self) {
        *self.position_history.entry(self.zobrist_hash).or_insert(0) += 1;
        self.repetition_count = *self.position_history.get(&self.zobrist_hash).unwrap_or(&0) as i32;
    }

    fn is_threefold_repetition(&self) -> bool {
        self.repetition_count >= 3
    }

    fn is_fifty_move_rule_draw(&self) -> bool {
        self.fifty_move_rule_counter >= 100
    }

    fn reset_position_history(&mut self) {
        self.position_history.clear();
        self.fifty_move_rule_counter = 0;
        self.repetition_count = 0;
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
        if row >= 0 && row < 10 && col >= 0 && col < 9 {
            self.board[row as usize][col as usize]
        } else {
            Piece::new()
        }
    }

    fn set_piece(&mut self, row: i32, col: i32, piece: Piece) {
        if row >= 0 && row < 10 && col >= 0 && col < 9 {
            self.board[row as usize][col as usize] = piece;
        }
    }

    fn is_valid_position(&self, row: i32, col: i32) -> bool {
        row >= 0 && row < 10 && col >= 0 && col < 9
    }

    fn move_piece(&mut self, mv: &Move, current_player: PieceColor) -> bool {
        let from_row = mv.from_row;
        let from_col = mv.from_col;
        let to_row = mv.to_row;
        let to_col = mv.to_col;

        if !self.is_valid_position(from_row, from_col) || !self.is_valid_position(to_row, to_col) {
            return false;
        }

        let from_piece = self.get_piece(from_row, from_col);
        let to_piece = self.get_piece(to_row, to_col);

        if from_piece.ptype == PieceType::Empty || from_piece.color == PieceColor::None {
            return false;
        }
        if from_piece.color != current_player {
            return false;
        }
        if to_piece.color == from_piece.color {
            return false;
        }
        if !self.is_move_legal(from_row, from_col, to_row, to_col) {
            return false;
        }

        let is_capture = to_piece.ptype != PieceType::Empty;
        let is_soldier_move = from_piece.ptype == PieceType::Soldier;
        if is_capture || is_soldier_move {
            self.fifty_move_rule_counter = 0;
        } else {
            self.fifty_move_rule_counter += 1;
        }

        self.update_zobrist_hash(from_row, from_col, to_row, to_col, to_piece);

        self.set_piece(to_row, to_col, from_piece);
        self.set_piece(from_row, from_col, Piece::new());

        self.record_position();
        true
    }

    fn is_move_legal(&self, from_row: i32, from_col: i32, to_row: i32, to_col: i32) -> bool {
        let from_piece = self.get_piece(from_row, from_col);
        let to_piece = self.get_piece(to_row, to_col);

        if from_piece.ptype == PieceType::Empty || from_piece.color == PieceColor::None {
            return false;
        }
        if from_piece.color == to_piece.color {
            return false;
        }

        match from_piece.ptype {
            PieceType::General => {
                self.is_general_move_legal(from_row, from_col, to_row, to_col, from_piece.color)
            }
            PieceType::Advisor => {
                self.is_advisor_move_legal(from_row, from_col, to_row, to_col, from_piece.color)
            }
            PieceType::Elephant => {
                self.is_elephant_move_legal(from_row, from_col, to_row, to_col, from_piece.color)
            }
            PieceType::Horse => self.is_horse_move_legal(from_row, from_col, to_row, to_col),
            PieceType::Chariot => self.is_chariot_move_legal(from_row, from_col, to_row, to_col),
            PieceType::Cannon => self.is_cannon_move_legal(from_row, from_col, to_row, to_col),
            PieceType::Soldier => {
                self.is_soldier_move_legal(from_row, from_col, to_row, to_col, from_piece.color)
            }
            _ => false,
        }
    }

    fn is_general_move_legal(
        &self,
        from_row: i32,
        from_col: i32,
        to_row: i32,
        to_col: i32,
        color: PieceColor,
    ) -> bool {
        let to_piece = self.get_piece(to_row, to_col);

        // “飞将”：将帅照面，可直接吃掉对方将/帅
        if to_piece.ptype == PieceType::General && to_piece.color != color {
            if from_col != to_col {
                return false;
            }
            let start_row = from_row.min(to_row) + 1;
            let end_row = from_row.max(to_row);
            for r in start_row..end_row {
                if self.get_piece(r, from_col).ptype != PieceType::Empty {
                    return false;
                }
            }
            return true;
        }

        // 九宫格限制
        if color == PieceColor::Red {
            if to_row < 7 || to_row > 9 || to_col < 3 || to_col > 5 {
                return false;
            }
        } else {
            if to_row < 0 || to_row > 2 || to_col < 3 || to_col > 5 {
                return false;
            }
        }

        let row_diff = (to_row - from_row).abs();
        let col_diff = (to_col - from_col).abs();
        (row_diff == 1 && col_diff == 0) || (row_diff == 0 && col_diff == 1)
    }

    fn is_advisor_move_legal(
        &self,
        from_row: i32,
        from_col: i32,
        to_row: i32,
        to_col: i32,
        color: PieceColor,
    ) -> bool {
        if color == PieceColor::Red {
            if to_row < 7 || to_row > 9 || to_col < 3 || to_col > 5 {
                return false;
            }
        } else {
            if to_row < 0 || to_row > 2 || to_col < 3 || to_col > 5 {
                return false;
            }
        }
        let row_diff = (to_row - from_row).abs();
        let col_diff = (to_col - from_col).abs();
        row_diff == 1 && col_diff == 1
    }

    fn is_elephant_move_legal(
        &self,
        from_row: i32,
        from_col: i32,
        to_row: i32,
        to_col: i32,
        color: PieceColor,
    ) -> bool {
        if color == PieceColor::Red && to_row < 5 {
            return false;
        }
        if color == PieceColor::Black && to_row > 4 {
            return false;
        }
        let row_diff = (to_row - from_row).abs();
        let col_diff = (to_col - from_col).abs();
        if row_diff != 2 || col_diff != 2 {
            return false;
        }
        let middle_row = (from_row + to_row) / 2;
        let middle_col = (from_col + to_col) / 2;
        self.get_piece(middle_row, middle_col).ptype == PieceType::Empty
    }

    fn is_horse_move_legal(&self, from_row: i32, from_col: i32, to_row: i32, to_col: i32) -> bool {
        let row_diff = (to_row - from_row).abs();
        let col_diff = (to_col - from_col).abs();

        if !((row_diff == 2 && col_diff == 1) || (row_diff == 1 && col_diff == 2)) {
            return false;
        }
        // 蹩马腿
        if row_diff == 2 {
            let middle_row = (from_row + to_row) / 2;
            if self.get_piece(middle_row, from_col).ptype != PieceType::Empty {
                return false;
            }
        } else {
            let middle_col = (from_col + to_col) / 2;
            if self.get_piece(from_row, middle_col).ptype != PieceType::Empty {
                return false;
            }
        }
        true
    }

    fn is_chariot_move_legal(&self, from_row: i32, from_col: i32, to_row: i32, to_col: i32) -> bool {
        if from_row != to_row && from_col != to_col {
            return false;
        }
        if from_row == to_row {
            let start_col = from_col.min(to_col) + 1;
            let end_col = from_col.max(to_col);
            for c in start_col..end_col {
                if self.get_piece(from_row, c).ptype != PieceType::Empty {
                    return false;
                }
            }
        } else {
            let start_row = from_row.min(to_row) + 1;
            let end_row = from_row.max(to_row);
            for r in start_row..end_row {
                if self.get_piece(r, from_col).ptype != PieceType::Empty {
                    return false;
                }
            }
        }
        true
    }

    fn is_cannon_move_legal(&self, from_row: i32, from_col: i32, to_row: i32, to_col: i32) -> bool {
        if from_row != to_row && from_col != to_col {
            return false;
        }
        let to_piece = self.get_piece(to_row, to_col);
        let mut pieces_between = 0;

        if from_row == to_row {
            let start_col = from_col.min(to_col) + 1;
            let end_col = from_col.max(to_col);
            for c in start_col..end_col {
                if self.get_piece(from_row, c).ptype != PieceType::Empty {
                    pieces_between += 1;
                }
            }
        } else {
            let start_row = from_row.min(to_row) + 1;
            let end_row = from_row.max(to_row);
            for r in start_row..end_row {
                if self.get_piece(r, from_col).ptype != PieceType::Empty {
                    pieces_between += 1;
                }
            }
        }

        if to_piece.ptype == PieceType::Empty {
            pieces_between == 0
        } else {
            pieces_between == 1
        }
    }

    fn is_soldier_move_legal(
        &self,
        from_row: i32,
        from_col: i32,
        to_row: i32,
        to_col: i32,
        color: PieceColor,
    ) -> bool {
        let row_diff = to_row - from_row;
        let col_diff = (to_col - from_col).abs();

        if color == PieceColor::Red && row_diff > 0 {
            return false;
        }
        if color == PieceColor::Black && row_diff < 0 {
            return false;
        }

        let is_crossed_river = (color == PieceColor::Red && from_row <= 4)
            || (color == PieceColor::Black && from_row >= 5);

        if is_crossed_river {
            (row_diff.abs() == 1 && col_diff == 0) || (row_diff.abs() == 0 && col_diff == 1)
        } else {
            row_diff.abs() == 1 && col_diff == 0
        }
    }

    fn is_in_check(&self, color: PieceColor) -> bool {
        let mut general_row = -1i32;
        let mut general_col = -1i32;
        for r in 0..10 {
            for c in 0..9 {
                let piece = self.board[r][c];
                if piece.ptype == PieceType::General && piece.color == color {
                    general_row = r as i32;
                    general_col = c as i32;
                    break;
                }
            }
            if general_row != -1 {
                break;
            }
        }
        if general_row == -1 {
            return false;
        }

        let opponent = if color == PieceColor::Red {
            PieceColor::Black
        } else {
            PieceColor::Red
        };
        for r in 0..10 {
            for c in 0..9 {
                let piece = self.board[r][c];
                if piece.color == opponent {
                    if self.is_move_legal(r as i32, c as i32, general_row, general_col) {
                        return true;
                    }
                }
            }
        }

        // 飞将情况
        let mut opp_row = -1i32;
        let mut opp_col = -1i32;
        for r in 0..10 {
            for c in 0..9 {
                let piece = self.board[r][c];
                if piece.ptype == PieceType::General && piece.color == opponent {
                    opp_row = r as i32;
                    opp_col = c as i32;
                    break;
                }
            }
            if opp_row != -1 {
                break;
            }
        }
        if opp_row == -1 {
            return false;
        }

        if general_col == opp_col {
            let start_row = general_row.min(opp_row) + 1;
            let end_row = general_row.max(opp_row);
            for r in start_row..end_row {
                if self.get_piece(r, general_col).ptype != PieceType::Empty {
                    return false;
                }
            }
            return true;
        }
        false
    }

    fn check_game_state(&self, current_player: PieceColor) -> GameState {
        let mut red_has_general = false;
        let mut black_has_general = false;
        for r in 0..10 {
            for c in 0..9 {
                let piece = self.board[r][c];
                if piece.ptype == PieceType::General {
                    if piece.color == PieceColor::Red {
                        red_has_general = true;
                    }
                    if piece.color == PieceColor::Black {
                        black_has_general = true;
                    }
                }
            }
        }
        if !red_has_general {
            return GameState::BlackWin;
        }
        if !black_has_general {
            return GameState::RedWin;
        }

        if self.is_in_check(current_player) {
            let legal = self.get_all_legal_moves(current_player);
            if legal.is_empty() {
                return if current_player == PieceColor::Red {
                    GameState::BlackWin
                } else {
                    GameState::RedWin
                };
            }
        }

        if self.is_threefold_repetition() {
            return GameState::Draw;
        }
        if self.is_fifty_move_rule_draw() {
            return GameState::Draw;
        }
        if self.is_insufficient_material() {
            return GameState::Draw;
        }
        GameState::Playing
    }

    fn is_insufficient_material(&self) -> bool {
        let mut red_pieces = 0;
        let mut black_pieces = 0;
        let mut red_attack = 0;
        let mut black_attack = 0;

        for r in 0..10 {
            for c in 0..9 {
                let piece = self.board[r][c];
                if piece.ptype != PieceType::Empty {
                    if piece.color == PieceColor::Red {
                        red_pieces += 1;
                        if matches!(
                            piece.ptype,
                            PieceType::Chariot | PieceType::Horse | PieceType::Cannon | PieceType::Soldier
                        ) {
                            red_attack += 1;
                        }
                    } else {
                        black_pieces += 1;
                        if matches!(
                            piece.ptype,
                            PieceType::Chariot | PieceType::Horse | PieceType::Cannon | PieceType::Soldier
                        ) {
                            black_attack += 1;
                        }
                    }
                }
            }
        }

        if red_pieces == 1 && black_pieces == 1 {
            return true;
        }
        if red_attack == 0 && black_attack == 0 {
            return true;
        }
        false
    }

    fn get_all_legal_moves(&self, color: PieceColor) -> Vec<Move> {
        let mut moves = Vec::new();
        for from_row in 0..10 {
            for from_col in 0..9 {
                let from_piece = self.board[from_row][from_col];
                if from_piece.color == color {
                    for to_row in 0..10 {
                        for to_col in 0..9 {
                            if self.is_move_legal(from_row as i32, from_col as i32, to_row as i32, to_col as i32)
                            {
                                let mut test = self.clone();
                                let test_move = Move::from_pos(
                                    from_row as i32,
                                    from_col as i32,
                                    to_row as i32,
                                    to_col as i32,
                                );
                                if test.move_piece(&test_move, color) && !test.is_in_check(color) {
                                    moves.push(test_move);
                                }
                            }
                        }
                    }
                }
            }
        }
        moves
    }

    fn get_game_phase(&self) -> GamePhase {
        let mut piece_count = 0;
        for i in 0..10 {
            for j in 0..9 {
                if self.board[i][j].ptype != PieceType::Empty {
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

    fn is_square_attacked(&self, row: i32, col: i32, attacker_color: PieceColor) -> bool {
        for r in 0..10 {
            for c in 0..9 {
                let piece = self.board[r][c];
                if piece.color == attacker_color && piece.ptype != PieceType::Empty {
                    if self.is_move_legal(r as i32, c as i32, row, col) {
                        return true;
                    }
                }
            }
        }
        false
    }

    // -----------------------------------------------------------------------
    // 评估函数
    // -----------------------------------------------------------------------

    fn evaluate(&self, perspective: PieceColor) -> i32 {
        let mut score = 0;

        for i in 0..10 {
            for j in 0..9 {
                let piece = self.board[i][j];
                if piece.ptype != PieceType::Empty && piece.color != PieceColor::None {
                    let piece_score = PIECE_VALUES[piece.ptype as usize];
                    let pos_bonus = self.get_position_bonus(i as i32, j as i32, piece.ptype, piece.color);
                    let total = piece_score + pos_bonus;

                    if (perspective == PieceColor::Red && piece.color == PieceColor::Red)
                        || (perspective == PieceColor::Black && piece.color == PieceColor::Black)
                    {
                        score += total;
                    } else {
                        score -= total;
                    }
                }
            }
        }

        score += self.evaluate_safety(perspective);

        let mobility = self.get_all_legal_moves(perspective).len() as i32;
        let opp = if perspective == PieceColor::Red {
            PieceColor::Black
        } else {
            PieceColor::Red
        };
        let opp_mobility = self.get_all_legal_moves(opp).len() as i32;
        score += (mobility - opp_mobility) * 3;

        if self.is_in_check(opp) {
            score += 50;
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
                if (color == PieceColor::Red && row <= 4) || (color == PieceColor::Black && row >= 5) {
                    bonus += 10;
                }
                if (color == PieceColor::Red && row <= 2) || (color == PieceColor::Black && row >= 7) {
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
        let opp = if perspective == PieceColor::Red {
            PieceColor::Black
        } else {
            PieceColor::Red
        };

        for i in 0..10 {
            for j in 0..9 {
                let piece = self.board[i][j];
                if piece.ptype != PieceType::Empty && piece.color == perspective {
                    let is_attacked = self.is_square_attacked(i as i32, j as i32, opp);
                    let is_protected = self.is_square_protected(i as i32, j as i32, perspective);

                    if is_attacked {
                        if !is_protected {
                            safety_score -= piece.value() / 2;
                        } else {
                            safety_score -= piece.value() / 10;
                        }
                    } else if is_protected {
                        safety_score += piece.value() / 20;
                    }
                }
            }
        }
        safety_score
    }

    fn is_square_protected(&self, row: i32, col: i32, defender_color: PieceColor) -> bool {
        for r in 0..10 {
            for c in 0..9 {
                let piece = self.board[r][c];
                if piece.color == defender_color && piece.ptype != PieceType::Empty {
                    if r as i32 == row && c as i32 == col {
                        continue;
                    }
                    if self.is_move_legal(r as i32, c as i32, row, col) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn evaluate_coordination(&self, perspective: PieceColor) -> i32 {
        let mut coordination = 0;
        let mut chariot_count = 0;
        let mut cannon_count = 0;
        let mut horse_count = 0;

        for i in 0..10 {
            for j in 0..9 {
                let piece = self.board[i][j];
                if piece.color == perspective {
                    match piece.ptype {
                        PieceType::Chariot => {
                            chariot_count += 1;
                            if (perspective == PieceColor::Red && i <= 4)
                                || (perspective == PieceColor::Black && i >= 5)
                            {
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
                            if i >= 3 && i <= 6 && j >= 3 && j <= 5 {
                                coordination += 10;
                            }
                        }
                        _ => {}
                    }
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
        let opp = if perspective == PieceColor::Red {
            PieceColor::Black
        } else {
            PieceColor::Red
        };

        let mut pawn_count = 0;
        let mut connected_pawns = 0;

        for i in 0..10 {
            for j in 0..9 {
                let piece = self.board[i][j];
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

                    if (perspective == PieceColor::Red && i <= 4)
                        || (perspective == PieceColor::Black && i >= 5)
                    {
                        pawn_score += 15;
                    }
                    if (perspective == PieceColor::Red && i <= 2)
                        || (perspective == PieceColor::Black && i >= 7)
                    {
                        pawn_score += 20;
                    }
                }
            }
        }

        let mut opp_pawn_count = 0;
        for i in 0..10 {
            for j in 0..9 {
                let piece = self.board[i][j];
                if piece.ptype == PieceType::Soldier && piece.color == opp {
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
                if piece.ptype != PieceType::Empty {
                    let value = PIECE_VALUES[piece.ptype as usize];
                    if piece.color == perspective {
                        score += value;
                    } else {
                        score -= value;
                    }
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

struct AI {
    color: PieceColor,
    difficulty: AIDifficulty,
    nodes_evaluated: i64,
    max_search_time: Duration,
    start_time: Instant,
    timeout: bool,
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
        }
    }

    fn get_best_move(&mut self, board: &Board) -> Move {
        self.nodes_evaluated = 0;
        self.timeout = false;

        let legal_moves = board.get_all_legal_moves(self.color);
        if legal_moves.is_empty() {
            return Move::new();
        }

        // 原版代码中四个难度统一设置为 1000ms
        match self.difficulty {
            AIDifficulty::Level1 => self.max_search_time = Duration::from_millis(1000),
            AIDifficulty::Level2 => self.max_search_time = Duration::from_millis(1000),
            AIDifficulty::Level3 => self.max_search_time = Duration::from_millis(1000),
            AIDifficulty::Level4 => self.max_search_time = Duration::from_millis(1000),
        }

        self.start_time = Instant::now();

        let mut best_move = match self.difficulty {
            AIDifficulty::Level1 => self.get_smart_heuristic_move(board, &legal_moves),
            AIDifficulty::Level2 => self.get_enhanced_minimax_move(board, &legal_moves, 2),
            AIDifficulty::Level3 => self.get_enhanced_minimax_move(board, &legal_moves, 3),
            AIDifficulty::Level4 => self.get_time_limited_advanced_move(board, &legal_moves),
        };

        // 未找到好着法时使用启发式
        if best_move.is_placeholder() {
            best_move = self.get_smart_heuristic_move(board, &legal_moves);
        }

        // 最后安全检查：确保移动不会立即被吃掉
        if !self.is_move_safe(board, &best_move) {
            for &mv in &legal_moves {
                if self.is_move_safe(board, &mv) {
                    best_move = mv;
                    best_move.score = self.evaluate_move_safety(board, &mv);
                    break;
                }
            }
        }

        best_move
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

    fn is_move_safe(&self, board: &Board, mv: &Move) -> bool {
        let opp = if self.color == PieceColor::Red {
            PieceColor::Black
        } else {
            PieceColor::Red
        };

        let mut test = board.clone();
        if !test.move_piece(mv, self.color) {
            return false;
        }
        if test.is_square_attacked(mv.to_row, mv.to_col, opp) {
            if !test.is_square_protected(mv.to_row, mv.to_col, self.color) {
                return false;
            }
        }
        true
    }

    fn evaluate_move_safety(&self, board: &Board, mv: &Move) -> i32 {
        let mut safety_score = 100;

        let moved_piece = board.get_piece(mv.from_row, mv.from_col);
        let target_piece = board.get_piece(mv.to_row, mv.to_col);
        let opp = if self.color == PieceColor::Red {
            PieceColor::Black
        } else {
            PieceColor::Red
        };

        let mut test = board.clone();
        if !test.move_piece(mv, self.color) {
            return -1000;
        }

        // 1. 移动后是否被将军
        if test.is_in_check(self.color) {
            safety_score -= 500;
        }

        // 2. 移动后棋子是否被攻击
        if test.is_square_attacked(mv.to_row, mv.to_col, opp) {
            safety_score -= 100;
            if !test.is_square_protected(mv.to_row, mv.to_col, self.color) {
                safety_score -= 200;
                if moved_piece.value() > 50 {
                    safety_score -= 100;
                }
            }
        }

        // 3. 吃子判断
        if target_piece.ptype != PieceType::Empty {
            if (moved_piece.value() as f64) < (target_piece.value() as f64) * 1.5 {
                safety_score += target_piece.value();
            }
        }

        // 4. 是否暴露重要棋子
        if self.exposes_important_piece(board, mv) {
            safety_score -= 150;
        }

        // 5. 移动到安全位置加分
        if !test.is_square_attacked(mv.to_row, mv.to_col, opp) {
            safety_score += 50;
        }

        safety_score
    }

    fn exposes_important_piece(&self, board: &Board, mv: &Move) -> bool {
        let moved_piece = board.get_piece(mv.from_row, mv.from_col);
        let opp = if self.color == PieceColor::Red {
            PieceColor::Black
        } else {
            PieceColor::Red
        };

        if matches!(
            moved_piece.ptype,
            PieceType::Chariot | PieceType::Cannon | PieceType::Horse
        ) {
            let mut general_row = -1i32;
            let mut general_col = -1i32;
            for r in 0..10 {
                for c in 0..9 {
                    let piece = board.board[r][c];
                    if piece.ptype == PieceType::General && piece.color == self.color {
                        general_row = r as i32;
                        general_col = c as i32;
                        break;
                    }
                }
                if general_row != -1 {
                    break;
                }
            }

            if general_row != -1 {
                let distance =
                    (mv.from_row - general_row).abs() + (mv.from_col - general_col).abs();
                if distance <= 3 {
                    let mut test = board.clone();
                    test.move_piece(mv, self.color);
                    if test.is_square_attacked(general_row, general_col, opp) {
                        return true;
                    }
                }
            }
        }
        false
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
            if target.ptype != PieceType::Empty {
                score += target.value() * 3;
            }

            // 移动后是否将军
            let mut test = board.clone();
            if test.move_piece(&mv, self.color) {
                let opp = if self.color == PieceColor::Red {
                    PieceColor::Black
                } else {
                    PieceColor::Red
                };
                if test.is_in_check(opp) {
                    score += 80;
                }
            }

            // 安全性评估
            score += self.evaluate_move_safety(board, &mv);

            // 棋子发展到好位置
            let moved_piece = board.get_piece(mv.from_row, mv.from_col);
            score += self.get_position_score(
                mv.to_row,
                mv.to_col,
                moved_piece.ptype,
                moved_piece.color,
            );

            // 协调性
            score += self.evaluate_move_coordination(board, &mv);

            // 避免重复移动
            if self.is_repeat_move(board, &mv) {
                score -= 30;
            }

            // 兵/卒前进奖励
            if moved_piece.ptype == PieceType::Soldier {
                if (self.color == PieceColor::Red && mv.to_row < mv.from_row)
                    || (self.color == PieceColor::Black && mv.to_row > mv.from_row)
                {
                    score += 20;
                }
            }

            scored_moves.push(ScoredMove { mv, score });
        }

        scored_moves.sort_by(|a, b| b.score.cmp(&a.score));

        for i in 0..scored_moves.len().min(5) {
            if self.is_move_safe(board, &scored_moves[i].mv) {
                return scored_moves[i].mv;
            }
        }
        scored_moves[0].mv
    }

    fn get_position_score(&self, row: i32, col: i32, ptype: PieceType, color: PieceColor) -> i32 {
        if ptype == PieceType::Horse {
            if row >= 3 && row <= 6 && col >= 3 && col <= 5 {
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
        } else if ptype == PieceType::Soldier {
            if (color == PieceColor::Red && row <= 4) || (color == PieceColor::Black && row >= 5) {
                return 15;
            }
        }
        0
    }

    fn evaluate_move_coordination(&self, board: &Board, mv: &Move) -> i32 {
        let mut coordination = 0;
        let moved_piece = board.get_piece(mv.from_row, mv.from_col);

        if matches!(moved_piece.ptype, PieceType::Chariot | PieceType::Cannon) {
            for r in 0..10 {
                for c in 0..9 {
                    if r as i32 == mv.from_row && c as i32 == mv.from_col {
                        continue;
                    }
                    let piece = board.board[r][c];
                    if piece.color == self.color
                        && matches!(piece.ptype, PieceType::Chariot | PieceType::Cannon)
                    {
                        if mv.to_row == r as i32 || mv.to_col == c as i32 {
                            coordination += 10;
                        }
                    }
                }
            }
        } else if moved_piece.ptype == PieceType::Horse {
            for r in 0..10 {
                for c in 0..9 {
                    if r as i32 == mv.from_row && c as i32 == mv.from_col {
                        continue;
                    }
                    let piece = board.board[r][c];
                    if piece.color == self.color
                        && matches!(piece.ptype, PieceType::Chariot | PieceType::Cannon)
                    {
                        let distance = (mv.to_row - r as i32).abs() + (mv.to_col - c as i32).abs();
                        if distance <= 3 {
                            coordination += 8;
                        }
                    }
                }
            }
        }
        coordination
    }

    fn is_repeat_move(&self, _board: &Board, _mv: &Move) -> bool {
        // 原版为简化实现，恒返回 false
        false
    }

    fn get_enhanced_minimax_move(&mut self, board: &Board, moves: &[Move], depth: i32) -> Move {
        if moves.is_empty() || depth <= 0 {
            return self.get_smart_heuristic_move(board, moves);
        }

        let mut best_move = moves[0];
        let mut best_score = i32::MIN;

        // 移动排序
        let mut scored_moves: Vec<ScoredMove> = Vec::new();
        for &mv in moves {
            let mut score = self.evaluate_move_safety(board, &mv);
            let target = board.get_piece(mv.to_row, mv.to_col);
            score += target.value() * 2;
            scored_moves.push(ScoredMove { mv, score });
        }
        scored_moves.sort_by(|a, b| b.score.cmp(&a.score));

        let search_limit = scored_moves.len().min(15);

        for i in 0..search_limit {
            let mv = scored_moves[i].mv;
            let mut test = board.clone();
            if !test.move_piece(&mv, self.color) {
                continue;
            }
            let score = self.enhanced_minimax(
                &test,
                depth - 1,
                false,
                i32::MIN,
                i32::MAX,
                true,
            );
            if score > best_score {
                best_score = score;
                best_move = mv;
                best_move.score = score;
            }
        }
        best_move
    }

    fn enhanced_minimax(
        &mut self,
        board: &Board,
        depth: i32,
        maximizing_player: bool,
        mut alpha: i32,
        mut beta: i32,
        use_quiescence: bool,
    ) -> i32 {
        self.nodes_evaluated += 1;

        if self.check_timeout() || depth == 0 {
            let mut eval = board.evaluate(self.color);
            if use_quiescence && depth == 0 && eval.abs() < 500 {
                eval = self.quiescence_search(board, 3, alpha, beta, maximizing_player);
            }
            return eval;
        }

        let current_player = if maximizing_player {
            self.color
        } else if self.color == PieceColor::Red {
            PieceColor::Black
        } else {
            PieceColor::Red
        };

        let state = board.check_game_state(current_player);
        if state != GameState::Playing {
            return self.get_terminal_score(state);
        }

        let legal_moves = board.get_all_legal_moves(current_player);
        if legal_moves.is_empty() {
            return if maximizing_player { -10000 } else { 10000 };
        }

        // 移动排序：吃子优先、安全性优先
        let mut scored_moves: Vec<ScoredMove> = Vec::new();
        for &mv in &legal_moves {
            let mut score = 0;
            let target = board.get_piece(mv.to_row, mv.to_col);
            if target.ptype != PieceType::Empty {
                score += 10000 + target.value() * 2;
            }
            score += self.evaluate_move_safety(board, &mv) / 10;
            scored_moves.push(ScoredMove { mv, score });
        }
        scored_moves.sort_by(|a, b| b.score.cmp(&a.score));

        if maximizing_player {
            let mut max_eval = i32::MIN;
            for sm in &scored_moves {
                let mut test = board.clone();
                test.move_piece(&sm.mv, current_player);
                let eval = self.enhanced_minimax(&test, depth - 1, false, alpha, beta, use_quiescence);
                max_eval = max_eval.max(eval);
                alpha = alpha.max(eval);
                if beta <= alpha {
                    break;
                }
            }
            max_eval
        } else {
            let mut min_eval = i32::MAX;
            for sm in &scored_moves {
                let mut test = board.clone();
                test.move_piece(&sm.mv, current_player);
                let eval = self.enhanced_minimax(&test, depth - 1, true, alpha, beta, use_quiescence);
                min_eval = min_eval.min(eval);
                beta = beta.min(eval);
                if beta <= alpha {
                    break;
                }
            }
            min_eval
        }
    }

    fn quiescence_search(
        &mut self,
        board: &Board,
        depth: i32,
        mut alpha: i32,
        mut beta: i32,
        maximizing_player: bool,
    ) -> i32 {
        self.nodes_evaluated += 1;

        let stand_pat = board.evaluate(self.color);
        if depth == 0 {
            return stand_pat;
        }

        if maximizing_player {
            if stand_pat >= beta {
                return beta;
            }
            if alpha < stand_pat {
                alpha = stand_pat;
            }
        } else {
            if stand_pat <= alpha {
                return alpha;
            }
            if beta > stand_pat {
                beta = stand_pat;
            }
        }

        // 只生成吃子着法
        let current_player = if maximizing_player {
            self.color
        } else if self.color == PieceColor::Red {
            PieceColor::Black
        } else {
            PieceColor::Red
        };
        let all_moves = board.get_all_legal_moves(current_player);
        let mut capture_moves: Vec<Move> = Vec::new();
        for mv in all_moves {
            let target = board.get_piece(mv.to_row, mv.to_col);
            if target.ptype != PieceType::Empty {
                capture_moves.push(mv);
            }
        }
        capture_moves.sort_by(|a, b| {
            board
                .get_piece(b.to_row, b.to_col)
                .value()
                .cmp(&board.get_piece(a.to_row, a.to_col).value())
        });

        if maximizing_player {
            for mv in capture_moves {
                let moved_piece = board.get_piece(mv.from_row, mv.from_col);
                let target = board.get_piece(mv.to_row, mv.to_col);
                // 吃子不利（被吃价值更高）则跳过
                if (moved_piece.value() as f64) > (target.value() as f64) * 1.5 {
                    continue;
                }
                let mut test = board.clone();
                test.move_piece(&mv, current_player);
                let score = self.quiescence_search(&test, depth - 1, alpha, beta, false);
                if score >= beta {
                    return beta;
                }
                if score > alpha {
                    alpha = score;
                }
            }
            alpha
        } else {
            for mv in capture_moves {
                let moved_piece = board.get_piece(mv.from_row, mv.from_col);
                let target = board.get_piece(mv.to_row, mv.to_col);
                if (moved_piece.value() as f64) > (target.value() as f64) * 1.5 {
                    continue;
                }
                let mut test = board.clone();
                test.move_piece(&mv, current_player);
                let score = self.quiescence_search(&test, depth - 1, alpha, beta, true);
                if score <= alpha {
                    return alpha;
                }
                if score < beta {
                    beta = score;
                }
            }
            beta
        }
    }

    fn get_time_limited_advanced_move(&mut self, board: &Board, moves: &[Move]) -> Move {
        if moves.is_empty() {
            return Move::new();
        }

        let mut scored_moves: Vec<ScoredMove> = Vec::new();
        for &mv in moves {
            let mut score = self.evaluate_move_safety(board, &mv) * 2;
            let target = board.get_piece(mv.to_row, mv.to_col);
            score += target.value() * 3;
            scored_moves.push(ScoredMove { mv, score });
        }
        scored_moves.sort_by(|a, b| b.score.cmp(&a.score));

        let search_limit = scored_moves.len().min(10);

        let mut best_move = scored_moves[0].mv;
        let mut best_score = i32::MIN;

        // 迭代深化
        for depth in 1..=5 {
            if self.check_timeout() {
                break;
            }
            let mut current_best_score = i32::MIN;
            let mut current_best_move = best_move;

            for i in 0..search_limit {
                if self.check_timeout() {
                    break;
                }
                let mv = scored_moves[i].mv;
                let mut test = board.clone();
                if !test.move_piece(&mv, self.color) {
                    continue;
                }
                let score = self.time_limited_advanced_search(
                    &test,
                    depth,
                    false,
                    i32::MIN,
                    i32::MAX,
                );
                if score > current_best_score {
                    current_best_score = score;
                    current_best_move = mv;
                    current_best_move.score = score;
                }
            }

            if !self.check_timeout() && current_best_score > best_score {
                best_score = current_best_score;
                best_move = current_best_move;
            }

            if best_score > 500 {
                break;
            }
        }
        best_move
    }

    fn time_limited_advanced_search(
        &mut self,
        board: &Board,
        depth: i32,
        maximizing_player: bool,
        mut alpha: i32,
        mut beta: i32,
    ) -> i32 {
        self.nodes_evaluated += 1;

        if self.check_timeout() || depth == 0 {
            return board.evaluate(self.color);
        }

        let current_player = if maximizing_player {
            self.color
        } else if self.color == PieceColor::Red {
            PieceColor::Black
        } else {
            PieceColor::Red
        };

        let state = board.check_game_state(current_player);
        if state != GameState::Playing {
            return self.get_terminal_score(state);
        }

        let legal_moves = board.get_all_legal_moves(current_player);
        if legal_moves.is_empty() {
            return if maximizing_player { -10000 } else { 10000 };
        }

        let mut scored_moves: Vec<ScoredMove> = Vec::new();
        for &mv in &legal_moves {
            let mut score = 0;
            let target = board.get_piece(mv.to_row, mv.to_col);
            score += target.value();
            score += self.evaluate_move_safety(board, &mv) / 20;
            scored_moves.push(ScoredMove { mv, score });
        }
        scored_moves.sort_by(|a, b| b.score.cmp(&a.score));

        if maximizing_player {
            let mut max_eval = i32::MIN;
            for sm in &scored_moves {
                if self.check_timeout() {
                    break;
                }
                let mut test = board.clone();
                test.move_piece(&sm.mv, current_player);
                let eval =
                    self.time_limited_advanced_search(&test, depth - 1, false, alpha, beta);
                max_eval = max_eval.max(eval);
                alpha = alpha.max(eval);
                if beta <= alpha {
                    break;
                }
            }
            max_eval
        } else {
            let mut min_eval = i32::MAX;
            for sm in &scored_moves {
                if self.check_timeout() {
                    break;
                }
                let mut test = board.clone();
                test.move_piece(&sm.mv, current_player);
                let eval =
                    self.time_limited_advanced_search(&test, depth - 1, true, alpha, beta);
                min_eval = min_eval.min(eval);
                beta = beta.min(eval);
                if beta <= alpha {
                    break;
                }
            }
            min_eval
        }
    }

    fn get_terminal_score(&self, state: GameState) -> i32 {
        if (state == GameState::RedWin && self.color == PieceColor::Red)
            || (state == GameState::BlackWin && self.color == PieceColor::Black)
        {
            10000
        } else if (state == GameState::RedWin && self.color == PieceColor::Black)
            || (state == GameState::BlackWin && self.color == PieceColor::Red)
        {
            -10000
        } else {
            0
        }
    }
}

// ---------------------------------------------------------------------------
// 游戏流程
// ---------------------------------------------------------------------------

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
            print!(" [50步规则: {}/100]", fifty_move_counter);
        }
        if self.board.is_in_check(self.current_player) {
            print!("{} [将军!]{}", red, cyan);
        }

        let red_eval = self.board.evaluate(PieceColor::Red);
        println!("\n局面评估: {}", if red_eval > 50 {
            "红方优势".to_string()
        } else if red_eval < -50 {
            "黑方优势".to_string()
        } else {
            "均势".to_string()
        });
        println!(" ({})", red_eval);
        println!("{}=== === === ==={}", cyan, reset);
    }

    fn start(&mut self) {
        let magenta = gamekit::color::MAGENTA;
        let yellow = gamekit::color::YELLOW;
        let reset = gamekit::color::RESET;

        println!("{}========================================={}", magenta, reset);
        println!("{}  中国象棋游戏（增强AI版，避免送棋）{}", magenta, reset);
        println!("{}  Rust 跨平台版 - 已修复移动敌方棋子问题{}", magenta, reset);
        println!("{}========================================={}\n", magenta, reset);

        println!("{}游戏规则说明:{}", yellow, reset);
        println!("1. 红方先手，黑方后手");
        println!("2. 将/帅只能在九宫格内移动");
        println!("3. 特殊规则：'飞将' - 当将帅在同一直线上且中间无子时，可以直接吃掉对方将帅");
        println!("4. 马走日，象走田，车走直线，炮隔山打牛");
        println!("5. 兵/卒过河前只能前进，过河后可左右移动");
        println!("6. 三次重复局面判和");
        println!("7. 50回合未吃子且无兵移动判和");
        println!("8. 双方无足够进攻子力判和");
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
            println!("1. 初级 - 简单启发式 (约1秒)");
            println!("2. 中级 - Minimax搜索深度2 (约1秒)");
            println!("3. 高级 - Minimax搜索深度3 (约1秒)");
            println!("4. 专家 - 时间限制搜索深度4+ (约1秒)\n");

            if self.red_is_ai {
                print!("{}选择红方AI难度 (1-4): {}", gamekit::color::RED, reset);
                let _ = std::io::stdout().flush();
                let d = read_int(4).unwrap_or(3);
                if (1..=4).contains(&d) {
                    self.set_ai_difficulty(PieceColor::Red, match d {
                        1 => AIDifficulty::Level1,
                        2 => AIDifficulty::Level2,
                        3 => AIDifficulty::Level3,
                        _ => AIDifficulty::Level4,
                    });
                }
            }
            if self.black_is_ai {
                print!("选择黑方AI难度 (1-4): ");
                let _ = std::io::stdout().flush();
                let d = read_int(4).unwrap_or(3);
                if (1..=4).contains(&d) {
                    self.set_ai_difficulty(PieceColor::Black, match d {
                        1 => AIDifficulty::Level1,
                        2 => AIDifficulty::Level2,
                        3 => AIDifficulty::Level3,
                        _ => AIDifficulty::Level4,
                    });
                }
            }
        }

        println!("{}\n游戏开始！{}", gamekit::color::CYAN, reset);

        if mode == 2 {
            println!("玩家: {}", if player_color == PieceColor::Red { "红方" } else { "黑方" });
            let ai_diff = if player_color == PieceColor::Red {
                self.black_ai_difficulty
            } else {
                self.red_ai_difficulty
            };
            println!("AI: {} (难度: {:?})", if player_color == PieceColor::Red { "黑方" } else { "红方" }, ai_diff as i32);
        } else {
            println!(
                "红方: {} (难度: {:?})",
                if self.red_is_ai { "AI" } else { "玩家" },
                self.red_ai_difficulty as i32
            );
            println!(
                "黑方: {} (难度: {:?})",
                if self.black_is_ai { "AI" } else { "玩家" },
                self.black_ai_difficulty as i32
            );
        }

        println!("\n提示：输入99 0查看特殊命令\n");

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
                if self.current_player == PieceColor::Red { "红方" } else { "黑方" },
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
                mv = ai.get_best_move(&self.board);
                let ai_duration = ai_start.elapsed();

                if mv.is_placeholder() {
                    println!("AI没有找到合法移动！");
                    self.state = if self.current_player == PieceColor::Red {
                        GameState::BlackWin
                    } else {
                        GameState::RedWin
                    };
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
                if target_piece.ptype == PieceType::General && target_piece.color != self.current_player {
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

            if self.board.move_piece(&mv, self.current_player) {
                self.move_history.push(mv);

                let mut move_str = format!(
                    "{}: {},{} -> {},{}",
                    if self.current_player == PieceColor::Red { "红方" } else { "黑方" },
                    mv.from_row,
                    mv.from_col,
                    mv.to_row,
                    mv.to_col
                );
                if to_piece.ptype != PieceType::Empty {
                    move_str.push_str(&format!(" 吃{}", to_piece.name()));
                } else {
                    move_str.push_str(&format!(" 移动{}", from_piece.name()));
                }
                if to_piece.ptype == PieceType::General && to_piece.color != self.current_player {
                    move_str.push_str(" [飞将!]");
                }
                self.game_log.push(move_str);

                self.state = self.board.check_game_state(self.current_player);

                if self.state == GameState::Draw {
                    if self.board.is_threefold_repetition() {
                        println!("{}三次重复局面，和棋！{}", yellow, reset);
                    } else if self.board.is_fifty_move_rule_draw() {
                        println!("{}50回合未吃子且无兵移动，和棋！{}", yellow, reset);
                    } else if self.board.is_insufficient_material() {
                        println!("{}双方无足够进攻子力，和棋！{}", yellow, reset);
                    }
                } else if self.state != GameState::Playing {
                    let target_piece = self.board.get_piece(mv.to_row, mv.to_col);
                    if target_piece.ptype == PieceType::General && target_piece.color != self.current_player {
                        println!("{}\n飞将成功！{}", magenta, reset);
                    }
                }

                self.current_player = if self.current_player == PieceColor::Red {
                    PieceColor::Black
                } else {
                    PieceColor::Red
                };
            } else {
                println!("{}非法移动，请重试！{}", gamekit::color::RED, reset);
            }
        }

        // 显示游戏结果
        self.board.display();
        println!("{}\n游戏结束！{}", magenta, reset);
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
        println!("重复局面次数: {}", self.board.get_repetition_count());
        println!("50步规则计数器: {}", self.board.get_fifty_move_counter());
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
                '０'..='９' => {
                    char::from_u32('0' as u32 + (c as u32 - '０' as u32)).unwrap_or(c)
                }
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
                        if piece.ptype != PieceType::Empty {
                            if piece.color == PieceColor::Red {
                                red_pieces += 1;
                            } else {
                                black_pieces += 1;
                            }
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
                        print!("红方AI当前难度: {:?}\n", self.red_ai_difficulty as i32);
                        print!("输入新难度 (1-4): ");
                        use std::io::Write;
                        let _ = std::io::stdout().flush();
                        let difficulty = read_int(4).unwrap_or(3);
                        if (1..=4).contains(&difficulty) {
                            self.red_ai_difficulty = match difficulty {
                                1 => AIDifficulty::Level1,
                                2 => AIDifficulty::Level2,
                                3 => AIDifficulty::Level3,
                                _ => AIDifficulty::Level4,
                            };
                            println!("红方AI难度已设置为: {:?}", self.red_ai_difficulty as i32);
                        } else {
                            println!("无效的难度级别");
                        }
                    }
                    if self.black_is_ai {
                        print!("黑方AI当前难度: {:?}\n", self.black_ai_difficulty as i32);
                        print!("输入新难度 (1-4): ");
                        use std::io::Write;
                        let _ = std::io::stdout().flush();
                        let difficulty = read_int(4).unwrap_or(3);
                        if (1..=4).contains(&difficulty) {
                            self.black_ai_difficulty = match difficulty {
                                1 => AIDifficulty::Level1,
                                2 => AIDifficulty::Level2,
                                3 => AIDifficulty::Level3,
                                _ => AIDifficulty::Level4,
                            };
                            println!("黑方AI难度已设置为: {:?}", self.black_ai_difficulty as i32);
                        } else {
                            println!("无效的难度级别");
                        }
                    }
                } else {
                    println!("当前没有AI玩家");
                }
            }
            6 => {
                println!("{}\n游戏规则说明:{}", yellow, reset);
                println!("1. 红方先手，黑方后手");
                println!("2. 将/帅只能在九宫格内移动");
                println!("3. 特殊规则：'飞将' - 当将帅在同一直线上且中间无子时，可以直接吃掉对方将帅");
                println!("4. 马走日，象走田，车走直线，炮隔山打牛");
                println!("5. 兵/卒过河前只能前进，过河后可左右移动");
                println!("6. 士/仕只能在九宫格内斜着移动");
                println!("7. 象/相不能过河");
                println!("8. 将军时，被将军方必须解除将军状态");
                println!("9. 无法解除将军则判负");
                println!("10. 三次重复局面判和");
                println!("11. 50回合未吃子且无兵移动判和");
                println!("12. 双方无足够进攻子力判和");
            }
            7 => {
                println!("{}局面信息:{}", cyan, reset);
                println!("当前局面重复次数: {}", self.board.get_repetition_count());
                println!("50步规则计数器: {} / 100", self.board.get_fifty_move_counter());
                print!("游戏阶段: ");
                match self.board.get_game_phase() {
                    GamePhase::Opening => println!("开局"),
                    GamePhase::Midgame => println!("中局"),
                    GamePhase::Endgame => println!("残局"),
                }
                if self.board.is_threefold_repetition() {
                    println!("警告：已达到三次重复局面！");
                }
                if self.board.is_fifty_move_rule_draw() {
                    println!("警告：已达到50回合未吃子且无兵移动！");
                }
                if self.board.is_insufficient_material() {
                    println!("警告：双方可能无足够进攻子力！");
                }
            }
            8 => {
                self.board.reset_position_history();
                println!("{}局面历史已清空！{}", green, reset);
            }
            9 => {
                println!("{}AI使用技术:{}", cyan, reset);
                println!("1. Alpha-Beta剪枝优化");
                println!("2. 启发式移动排序（避免送棋）");
                println!("3. 静态交换评估（Quiescence Search）");
                println!("4. 迭代深化搜索");
                println!("5. 时间限制搜索");
                println!("6. 增强的评估函数（安全性优先）");
                println!("7. 局面哈希和重复检测");
                println!("8. 棋子安全性评估");
                println!("9. 棋子协调性评估");
                println!("10. 兵/卒结构评估");
            }
            _ => println!("未知命令"),
        }
    }
}

/// 读取一个 1..=max 范围内的整数（读取失败或 EOF 返回 None）
fn read_int(max: i32) -> Option<i32> {
    let line = gamekit::read_line_cooked()?;
    let v = line.trim().parse::<i32>().ok()?;
    if (1..=max).contains(&v) {
        Some(v)
    } else {
        Some(v)
    }
}

fn main() {
    gamekit::init();
    let mut game = Game::new();
    game.start();

    println!("{}\n感谢游玩中国象棋增强AI版！{}", gamekit::color::MAGENTA, gamekit::color::RESET);
}
