//! 逃离学校 8.1 —— 原 C++（Windows 版）的 Rust 跨平台移植
//!
//! 原版依赖 windows.h（GetAsyncKeyState / SetConsoleTitle / system("cls") /
//! Sleep），本移植全部改为跨平台实现：
//! - 实时按键：crossterm 原始模式 + 事件轮询（替代 GetAsyncKeyState）
//! - 清屏/光标/窗口标题：ANSI 转义序列
//! - 延时：std::thread::sleep
//!
//! 玩法与原版一致：WSAD 移动、数字键 1/2/3 切换速度、Q 存档、
//! 失败后可回档；生成“你的游戏过程.cpp”回放文件的功能保留，
//! 但生成的 C++ 代码改为跨平台的 ANSI 版本（不再依赖 windows.h）。

use gamekit;
use gamekit::{FrameKeys, RawMode};
use std::io::Write;

// ---------------------------------------------------------------------------
// 基础类型
// ---------------------------------------------------------------------------

type Map = Vec<Vec<String>>;

/// 生成一张 h×w 的地图（含下标 0 的未使用行列，原版为 1 起始索引）
fn make_map(h: usize, w: usize) -> Map {
    vec![vec!["  ".to_string(); w]; h]
}

fn map_of(world: &mut World, id: i32) -> &mut Map {
    match id {
        1 => &mut world.a1,
        2 => &mut world.a2,
        3 => &mut world.a3,
        4 => &mut world.a4,
        _ => &mut world.a5,
    }
}

#[derive(Clone)]
struct I {
    x: i32,
    y: i32,
    s: String,
}

#[derive(Clone)]
struct Save {
    name: String,
    g: i32,
    x: i32,
    y: i32,
    a: String,
    a1: Map,
    a2: Map,
    a3: Map,
    a4: Map,
    a5: Map,
    wd: String,
    a2npc: Vec<I>,
    a4npc: Vec<I>,
    a5npc: Vec<I>,
    li: f64,
    bli: f64,
    start_end: Vec<String>,
}

/// 与地图无关的核心状态
struct Core {
    li: f64,
    bli: f64,
    way: i32,
    start_end: Vec<String>,
    wd: String,
    cd: Vec<Save>,
    // 回档待恢复的位置（原版 CD_o / CD_x / CD_y / CD_a 机制）
    cd_o: bool,
    cd_x: i32,
    cd_y: i32,
    cd_a: String,
}

/// 地图与 NPC（保存/加载时整体拷贝）
struct World {
    a1: Map,
    a2: Map,
    a3: Map,
    a4: Map,
    a5: Map,
    a2npc: Vec<I>,
    a4npc: Vec<I>,
    a5npc: Vec<I>,
}

// ---------------------------------------------------------------------------
// 地图生成
// ---------------------------------------------------------------------------

fn ready(world: &mut World, core: &mut Core) {
    core.li = 1.0;
    core.bli = 1.0;
    core.wd = " ".to_string();

    let a = "墙".to_string();
    let b = "床".to_string();
    let c = "柜".to_string();
    let d = "门".to_string();
    let e = "消".to_string();
    let f = "电".to_string();
    let o = "O ".to_string();
    let space = "  ".to_string();

    // 生成地图1（宿舍房间）
    world.a1 = make_map(8, 8);
    for i in 1..=7 {
        world.a1[1][i] = a.clone();
        world.a1[7][i] = a.clone();
    }
    for i in 2..=5 {
        world.a1[i][1] = a.clone();
        world.a1[i][7] = a.clone();
        world.a1[i][2] = b.clone();
        world.a1[i][6] = b.clone();
    }
    world.a1[6][1] = a.clone();
    world.a1[6][7] = a.clone();
    world.a1[6][2] = c.clone();
    world.a1[6][6] = c.clone();
    world.a1[7][4] = d.clone();

    // 生成地图2（宿舍走廊）
    world.a2 = make_map(19, 13);
    for i in 3..=9 {
        world.a2[1][i] = a.clone();
        world.a2[18][i] = a.clone();
    }
    world.a2[1][8] = c.clone();
    let mut i = 2;
    while i <= 9 {
        world.a2[i][3] = a.clone();
        world.a2[i][9] = a.clone();
        if i == 6 {
            i += 2;
        }
        i += 1;
    }
    world.a2[6][2] = a.clone();
    world.a2[6][1] = a.clone();
    for i in 7..=8 {
        world.a2[i][1] = a.clone();
        world.a2[i][9] = a.clone();
    }
    world.a2[7][1] = c.clone();
    world.a2[8][8] = e.clone();
    world.a2[9][2] = a.clone();
    world.a2[9][1] = a.clone();
    world.a2[9][10] = a.clone();
    world.a2[9][11] = a.clone();
    world.a2[9][12] = a.clone();
    for i in 10..=13 {
        world.a2[i][3] = a.clone();
        world.a2[i][12] = a.clone();
        if i >= 12 {
            world.a2[i][9] = a.clone();
        }
    }
    for i in 12..=13 {
        world.a2[i][10] = a.clone();
        world.a2[i][11] = a.clone();
    }
    world.a2[12][11] = o.clone();
    world.a2[13][11] = f.clone();
    for i in 14..=17 {
        world.a2[i][3] = a.clone();
        world.a2[i][9] = a.clone();
    }
    world.a2[16][9] = o.clone();
    world.a2[15][10] = a.clone();
    world.a2[17][10] = a.clone();
    world.a2[16][10] = d.clone();
    world.a2[9][4] = a.clone();
    world.a2[15][8] = a.clone();
    world.a2[7][3] = a.clone();

    // 地图2 的宿（NPC）
    world.a2npc = vec![
        I {
            x: 7,
            y: 2,
            s: "宿".to_string(),
        },
        I {
            x: 9,
            y: 8,
            s: "宿".to_string(),
        },
        I {
            x: 2,
            y: 8,
            s: "宿".to_string(),
        },
    ];
    for i in 0..3 {
        let npc = &world.a2npc[i];
        world.a2[npc.x as usize][npc.y as usize] = npc.s.clone();
    }

    // 生成地图3（楼内大厅）
    world.a3 = make_map(6, 8);
    for i in 1..=6 {
        world.a3[1][i] = a.clone();
        world.a3[5][i] = a.clone();
    }
    for i in (2..=4).step_by(2) {
        world.a3[i][1] = a.clone();
        world.a3[i][6] = a.clone();
        world.a3[i][7] = a.clone();
    }
    world.a3[3][1] = d.clone();
    world.a3[3][7] = f.clone();
    world.a3[3][6] = o.clone();

    // 生成地图4（校园）
    world.a4 = make_map(31, 31);
    for i in 1..=30 {
        world.a4[1][i] = a.clone();
        world.a4[30][i] = a.clone();
    }
    for i in 2..=29 {
        world.a4[i][1] = a.clone();
        world.a4[i][30] = a.clone();
    }
    world.a4[1][14] = "大".to_string();
    world.a4[14][30] = "大".to_string();
    world.a4[1][15] = d.clone();
    world.a4[15][30] = d.clone();
    for i in 5..=10 {
        world.a4[4][i] = a.clone();
        world.a4[12][i] = a.clone();
    }
    for i in 5..=11 {
        world.a4[i][5] = a.clone();
        world.a4[i][10] = a.clone();
    }
    world.a4[12][8] = space.clone();
    for i in 19..=23 {
        world.a4[4][i] = a.clone();
        world.a4[11][i] = a.clone();
    }
    for i in 5..=10 {
        world.a4[i][19] = a.clone();
        world.a4[i][23] = a.clone();
    }
    world.a4[10][19] = d.clone();
    world.a4[12][8] = o.clone();
    world.a4[7][7] = "教".to_string();
    world.a4[8][7] = "学".to_string();
    world.a4[9][7] = "楼".to_string();
    world.a4[25][2] = "楼".to_string();
    world.a4[7][21] = "食".to_string();
    world.a4[8][21] = "堂".to_string();
    world.a4[6][13] = a.clone();
    world.a4[6][16] = a.clone();
    world.a4[11][13] = a.clone();
    world.a4[11][16] = a.clone();
    for i in 24..=26 {
        world.a4[5][i] = a.clone();
        world.a4[11][i] = a.clone();
    }
    for i in 6..=10 {
        world.a4[i][26] = a.clone();
    }
    world.a4[6][24] = "女".to_string();
    world.a4[19][24] = "男".to_string();
    world.a4[7][24] = "生".to_string();
    world.a4[20][24] = "生".to_string();
    world.a4[8][24] = "宿".to_string();
    world.a4[21][24] = "宿".to_string();
    world.a4[9][24] = "舍".to_string();
    world.a4[22][24] = "舍".to_string();
    for i in 2..=6 {
        world.a4[17][i] = a.clone();
        world.a4[24][i] = a.clone();
    }
    for i in 18..=28 {
        world.a4[i][6] = a.clone();
    }
    world.a4[19][3] = "图".to_string();
    world.a4[20][3] = "书".to_string();
    world.a4[21][3] = "馆".to_string();
    world.a4[17][5] = d.clone();
    for i in 10..=16 {
        world.a4[16][i] = a.clone();
        world.a4[23][i] = a.clone();
    }
    for i in 17..=22 {
        world.a4[i][10] = a.clone();
        world.a4[i][16] = a.clone();
    }
    world.a4[18][13] = "明".to_string();
    world.a4[19][13] = "礼".to_string();
    world.a4[20][13] = "堂".to_string();
    world.a4[16][15] = d.clone();
    world.a4[23][15] = d.clone();
    for i in 25..=26 {
        world.a4[i][3] = a.clone();
    }
    for i in 2..=4 {
        world.a4[28][i] = a.clone();
    }
    for i in 20..=26 {
        world.a4[18][i] = a.clone();
        world.a4[28][i] = a.clone();
    }
    for i in 19..=27 {
        world.a4[i][20] = a.clone();
        world.a4[i][26] = a.clone();
    }
    for i in 19..=23 {
        world.a4[i][23] = a.clone();
    }
    for i in 23..=25 {
        world.a4[24][i] = a.clone();
    }
    world.a4[23][22] = "游".to_string();
    world.a4[24][22] = "泳".to_string();
    world.a4[25][22] = "馆".to_string();
    world.a4[9][26] = o.clone();
    world.a4[22][26] = o.clone();
    world.a4[24][14] = a.clone();
    for i in 10..=16 {
        world.a4[25][i] = a.clone();
        world.a4[28][i] = a.clone();
    }
    for i in 26..=27 {
        world.a4[i][10] = a.clone();
        world.a4[i][16] = a.clone();
    }
    world.a4[19][20] = d.clone();
    world.a4[26][26] = d.clone();
    world.a4[26][2] = o.clone();
    world.a4[26][11] = "体".to_string();
    world.a4[26][12] = "育".to_string();
    world.a4[26][13] = "办".to_string();
    world.a4[26][14] = "公".to_string();
    world.a4[26][15] = "室".to_string();
    world.a4[25][13] = d.clone();
    world.a4[1][13] = "亭".to_string();
    world.a4[13][30] = "亭".to_string();

    // 地图4 NPC
    world.a4npc = vec![
        I {
            x: 1,
            y: 13,
            s: "保".to_string(),
        },
        I {
            x: 13,
            y: 30,
            s: "保".to_string(),
        },
        I {
            x: 5,
            y: 12,
            s: "师".to_string(),
        },
        I {
            x: 8,
            y: 17,
            s: "师".to_string(),
        },
        I {
            x: 10,
            y: 28,
            s: "师".to_string(),
        },
        I {
            x: 14,
            y: 19,
            s: "师".to_string(),
        },
        I {
            x: 23,
            y: 8,
            s: "师".to_string(),
        },
        I {
            x: 28,
            y: 19,
            s: "师".to_string(),
        },
        I {
            x: 12,
            y: 3,
            s: "师".to_string(),
        },
        I {
            x: 2,
            y: 14,
            s: "校".to_string(),
        },
    ];
    for i in 2..=9 {
        let npc = &world.a4npc[i];
        world.a4[npc.x as usize][npc.y as usize] = npc.s.clone();
    }

    // 生成地图5（图书馆）
    world.a5 = make_map(14, 13);
    for i in 1..=12 {
        world.a5[1][i] = a.clone();
        world.a5[13][i] = a.clone();
    }
    for i in 2..=12 {
        world.a5[i][1] = a.clone();
        world.a5[i][12] = a.clone();
    }
    world.a5[1][11] = space.clone();
    for i in (4..=10).step_by(2) {
        world.a5[2][i] = a.clone();
        world.a5[3][i] = a.clone();
    }
    for i in (3..=9).step_by(2) {
        for j in 5..=9 {
            world.a5[j][i] = a.clone();
        }
    }
    for i in 3..=10 {
        world.a5[7][i] = a.clone();
    }
    for i in (4..=10).step_by(2) {
        world.a5[11][i] = a.clone();
        world.a5[12][i] = a.clone();
    }
    world.a5[13][2] = c.clone();
    world.a5[13][11] = c.clone();
    world.a5[11][2] = a.clone();

    // 地图5 NPC
    world.a5npc = vec![
        I {
            x: 4,
            y: 2,
            s: "师".to_string(),
        },
        I {
            x: 12,
            y: 2,
            s: "师".to_string(),
        },
    ];
    for i in 0..2 {
        let npc = &world.a5npc[i];
        world.a5[npc.x as usize][npc.y as usize] = npc.s.clone();
    }
}

// ---------------------------------------------------------------------------
// 渲染
// ---------------------------------------------------------------------------

fn coutt(core: &mut Core, map: &Map, _x: i32, _y: i32) {
    let h = map.len();
    let w = map[0].len();

    let mut frame = String::new();
    frame.push_str("\x1b[2J\x1b[H");
    if core.bli > 1000.0 {
        frame.push_str("体力：∞/∞\r\n");
        core.li = core.bli;
    } else {
        frame.push_str(&format!("体力：{:.1}/{:.1}\r\n", core.li, core.bli));
    }
    frame.push_str("当前速度:");
    if core.way == 1 {
        frame.push_str("走路\r\n");
    } else if core.way == 2 {
        frame.push_str("跑步\r\n");
    } else {
        frame.push_str("疾跑\r\n");
    }
    for i in 1..h {
        for j in 1..w {
            frame.push_str(&map[i][j]);
        }
        frame.push_str("\r\n");
    }

    print!("{}", frame);
    let _ = std::io::stdout().flush();
    core.start_end.push(frame);
}

// ---------------------------------------------------------------------------
// 移动与速度
// ---------------------------------------------------------------------------

fn move_player(
    core: &mut Core,
    world: &mut World,
    g: i32,
    map_id: i32,
    x: &mut i32,
    y: &mut i32,
    an: &mut bool,
    c: &mut i32,
    a: &mut String,
    keys: &FrameKeys,
) {
    // 回档：恢复存档位置（原版 CD_o 机制）
    if core.cd_o {
        *x = core.cd_x;
        *y = core.cd_y;
        *a = core.cd_a.clone();
        core.cd_o = false;
        *an = true;
        return;
    }

    if keys.w {
        let map = map_of(world, map_id);
        let (nx, ny) = (*x - 1, *y);
        if map[nx as usize][ny as usize] == "  " || map[nx as usize][ny as usize] == "O " {
            map[*x as usize][*y as usize] = a.clone();
            *x -= 1;
            *a = map[nx as usize][ny as usize].clone();
            map[nx as usize][ny as usize] = "我".to_string();
            *an = true;
            *c -= 1;
        }
    } else if keys.s {
        let map = map_of(world, map_id);
        let (nx, ny) = (*x + 1, *y);
        if map[nx as usize][ny as usize] == "  " || map[nx as usize][ny as usize] == "O " {
            map[*x as usize][*y as usize] = a.clone();
            *x += 1;
            *a = map[nx as usize][ny as usize].clone();
            map[nx as usize][ny as usize] = "我".to_string();
            *an = true;
            *c -= 1;
        }
    } else if keys.a {
        let map = map_of(world, map_id);
        let (nx, ny) = (*x, *y - 1);
        if map[nx as usize][ny as usize] == "  "
            || map[nx as usize][ny as usize] == "O "
            || map[nx as usize][ny as usize] == "床"
        {
            map[*x as usize][*y as usize] = a.clone();
            *y -= 1;
            *a = map[nx as usize][ny as usize].clone();
            map[nx as usize][ny as usize] = "我".to_string();
            *an = true;
            *c -= 1;
        }
    } else if keys.d {
        let map = map_of(world, map_id);
        let (nx, ny) = (*x, *y + 1);
        if map[nx as usize][ny as usize] == "  "
            || map[nx as usize][ny as usize] == "O "
            || map[nx as usize][ny as usize] == "床"
        {
            map[*x as usize][*y as usize] = a.clone();
            *y += 1;
            *a = map[nx as usize][ny as usize].clone();
            map[nx as usize][ny as usize] = "我".to_string();
            *an = true;
            *c -= 1;
        }
    } else if keys.q {
        // 存档
        if g == 0 || *c != core.way {
            print!("无法存档");
            let _ = std::io::stdout().flush();
            return;
        }
        gamekit::clear_screen();
        print!("请输入存档名字:");
        let _ = std::io::stdout().flush();
        let name = gamekit::read_line_raw("请输入存档名字:");

        let save = Save {
            name,
            g,
            x: *x,
            y: *y,
            a: a.clone(),
            a1: world.a1.clone(),
            a2: world.a2.clone(),
            a3: world.a3.clone(),
            a4: world.a4.clone(),
            a5: world.a5.clone(),
            wd: core.wd.clone(),
            a2npc: world.a2npc.clone(),
            a4npc: world.a4npc.clone(),
            a5npc: world.a5npc.clone(),
            li: core.li,
            bli: core.bli,
            start_end: core.start_end.clone(),
        };
        core.cd.push(save);
        *an = true;
    }
}

fn speed(core: &mut Core, c: &mut i32, keys: &FrameKeys) {
    if keys.key1 {
        if *c == core.way {
            core.way = 1;
            *c = 1;
            print!("已切换为走路\r\n");
        } else {
            print!("正在运动,无法切换\r\n");
        }
        gamekit::sleep_ms(60);
    } else if keys.key2 {
        if *c == core.way {
            core.way = 2;
            *c = 2;
            print!("已切换为跑步\r\n");
        } else {
            print!("正在运动,无法切换\r\n");
        }
        gamekit::sleep_ms(60);
    } else if keys.key3 {
        if *c == core.way {
            core.way = 3;
            *c = 3;
            print!("已切换为疾跑\r\n");
        } else {
            print!("正在运动,无法切换\r\n");
        }
        gamekit::sleep_ms(60);
    }
}

// ---------------------------------------------------------------------------
// 各关卡
// ---------------------------------------------------------------------------

fn f1(core: &mut Core, world: &mut World, o: i32) -> i32 {
    gamekit::sleep_ms(500);
    core.way = 1;
    let mut x: i32;
    let mut y: i32;
    let mut c = 1;
    let mut an: bool;
    let mut a: String;

    if o == 1 {
        x = 4;
        y = 6;
        world.a1[x as usize][y as usize] = "我".to_string();
        a = "床".to_string();
        let mut s = 70;
        an = true;
        while s > 0 {
            s -= 1;
            let keys = gamekit::drain_keys();
            move_player(
                core, world, 0, 1, &mut x, &mut y, &mut an, &mut c, &mut a, &keys,
            );
            if an {
                core.li = (core.li + 0.1).min(core.bli);
                coutt(core, &world.a1, x, y);
                an = false;
                if x == 6 && y == 3 {
                    a = "无".to_string();
                    print!("获得1瓶饮料,体力上限+1");
                    let _ = std::io::stdout().flush();
                    core.bli += 1.0;
                    core.li += 1.0;
                }
                if x == 6 && y == 5 {
                    a = "无".to_string();
                    print!("获得1瓶饮料,体力上限+1");
                    let _ = std::io::stdout().flush();
                    core.bli += 1.0;
                    core.li += 1.0;
                }
            }
            gamekit::sleep_ms(80);
        }
        world.a1[7][4] = "宿".to_string();
        coutt(core, &world.a1, x, y);
        if x != 4 || y != 6 {
            print!("失败原因：你被宿发现没有上自己的床了\r\n");
            return 0;
        }
        gamekit::sleep_ms(500);
        world.a1[7][4] = "O ".to_string();
        world.a1[6][4] = "宿".to_string();
        coutt(core, &world.a1, x, y);
        gamekit::sleep_ms(500);
        world.a1[6][4] = "  ".to_string();
        world.a1[6][3] = "宿".to_string();
        coutt(core, &world.a1, x, y);
        print!("宿检查中...");
        let _ = std::io::stdout().flush();
        gamekit::sleep_ms(1000);
        world.a1[6][3] = "无".to_string();
        world.a1[6][4] = "宿".to_string();
        coutt(core, &world.a1, x, y);
        gamekit::sleep_ms(500);
        world.a1[6][4] = "  ".to_string();
        world.a1[6][5] = "宿".to_string();
        coutt(core, &world.a1, x, y);
        print!("宿检查中...");
        let _ = std::io::stdout().flush();
        gamekit::sleep_ms(1000);
        world.a1[6][5] = "无".to_string();
        world.a1[6][4] = "宿".to_string();
        coutt(core, &world.a1, x, y);
        gamekit::sleep_ms(500);
        world.a1[6][4] = "  ".to_string();
        world.a1[7][4] = "宿".to_string();
        coutt(core, &world.a1, x, y);
        gamekit::sleep_ms(500);
        world.a1[7][4] = "O ".to_string();
    } else if o == 2 {
        // 从地图2返回
        x = 6;
        y = 4;
        world.a1[x as usize][y as usize] = "我".to_string();
        a = "  ".to_string();
    } else {
        // o == 0：回档后由 cd_o 机制恢复位置
        x = 0;
        y = 0;
        a = String::new();
    }

    an = true;
    loop {
        let keys = gamekit::drain_keys();
        move_player(
            core, world, 1, 1, &mut x, &mut y, &mut an, &mut c, &mut a, &keys,
        );
        if an {
            core.li = (core.li + 0.1).min(core.bli);
            coutt(core, &world.a1, x, y);
            an = false;
        }
        if x == 7 && y == 4 {
            world.a1[x as usize][y as usize] = "O ".to_string();
            return 2;
        }
        gamekit::sleep_ms(80);
    }
}

fn f2(core: &mut Core, world: &mut World, o: i32) -> i32 {
    gamekit::sleep_ms(500);
    let mut x: i32;
    let mut y: i32;
    let mut a = "  ".to_string();
    if o == 1 {
        x = 16;
        y = 8;
        world.a2[x as usize][y as usize] = "我".to_string();
    } else if o == 3 {
        // 从地图3返回
        x = 11;
        y = 11;
        world.a2[x as usize][y as usize] = "我".to_string();
    } else {
        // o == 0：回档后由 cd_o 机制恢复位置
        x = 0;
        y = 0;
    }
    let mut an = true;
    let mut c = core.way;
    loop {
        if c == 0 {
            // 体力变化
            if core.way == 1 {
                core.li = (core.li + 0.1).min(core.bli);
            } else if core.way == 2 {
                core.li = (core.li - 0.5).max(0.0);
            } else {
                core.li = (core.li - 1.0).max(0.0);
            }
            // 宿移动
            for i in 0..3 {
                let npc = world.a2npc[i].clone();
                let (nx, ny) = (npc.x, npc.y);
                if (npc.x - x).abs() + (npc.y - y).abs() - 1 <= 3 {
                    let dx = npc.x - x;
                    let dy = npc.y - y;
                    let sx = if dx != 0 { dx / dx.abs() } else { 0 };
                    let sy = if dy != 0 { dy / dy.abs() } else { 0 };
                    if (npc.x - x).abs() >= (npc.y - y).abs()
                        && world.a2[(nx - sx) as usize][ny as usize] == "  "
                        && npc.x != x
                    {
                        world.a2[nx as usize][ny as usize] = "  ".to_string();
                        world.a2npc[i].x -= sx;
                        let npc2 = world.a2npc[i].clone();
                        world.a2[npc2.x as usize][npc2.y as usize] = npc2.s.clone();
                        core.li -= 0.1;
                    } else if world.a2[nx as usize][(ny - sy) as usize] == "  " && npc.y != y {
                        world.a2[nx as usize][ny as usize] = "  ".to_string();
                        world.a2npc[i].y -= sy;
                        let npc2 = world.a2npc[i].clone();
                        world.a2[npc2.x as usize][npc2.y as usize] = npc2.s.clone();
                        core.li -= 0.1;
                    } else if world.a2[(nx - sx) as usize][ny as usize] == "  " && npc.x != x {
                        world.a2[nx as usize][ny as usize] = "  ".to_string();
                        world.a2npc[i].x -= sx;
                        let npc2 = world.a2npc[i].clone();
                        world.a2[npc2.x as usize][npc2.y as usize] = npc2.s.clone();
                        core.li -= 0.1;
                    }
                }
                let npc = world.a2npc[i].clone();
                if (npc.x - x).abs() <= 1 && (npc.y - y).abs() <= 1 {
                    coutt(core, &world.a2, x, y);
                    print!("你被宿抓住了\r\n");
                    return 0;
                }
            }
            c = core.way;
        }
        if an {
            if core.li <= 0.0 {
                coutt(core, &world.a2, x, y);
                print!("你体力不足\r\n");
                return 0;
            }
            coutt(core, &world.a2, x, y);
            an = false;
            if x == 7 && y == 2 {
                core.wd += "宿舍门禁卡 ";
                a = "无".to_string();
                print!("获得宿舍门禁卡\r\n");
            }
            if x == 2 && y == 8 {
                core.wd += "体育办公室钥匙 ";
                a = "无".to_string();
                print!("获得体育办公室钥匙\r\n");
            }
            if c == core.way {
                print!("按数字键切换速度\r\n");
            }
        }
        let keys = gamekit::drain_keys();
        move_player(
            core, world, 2, 2, &mut x, &mut y, &mut an, &mut c, &mut a, &keys,
        );
        speed(core, &mut c, &keys);
        if x == 16 && y == 9 {
            world.a2[x as usize][y as usize] = "O ".to_string();
            return 1;
        }
        if x == 12 && y == 11 {
            world.a2[x as usize][y as usize] = "O ".to_string();
            return 3;
        }
        gamekit::sleep_ms(80);
    }
}

fn f3(core: &mut Core, world: &mut World, o: i32) -> i32 {
    gamekit::sleep_ms(500);
    core.way = 1;
    let mut x: i32;
    let mut y: i32;
    let mut a = "  ".to_string();
    if o == 2 {
        x = 3;
        y = 5;
        world.a3[x as usize][y as usize] = "我".to_string();
    } else if o == 4 {
        // 从地图4返回
        x = 3;
        y = 2;
        world.a3[x as usize][y as usize] = "我".to_string();
    } else {
        // o == 3（原版未定义行为，实际不可达）或 o == 0（回档）：
        // 定义落点；回档时位置随后由 cd_o 恢复
        x = 3;
        y = 2;
        if o != 3 {
            world.a3[x as usize][y as usize] = "我".to_string();
        }
    }
    let mut an = true;
    let mut c = 1;
    loop {
        if an {
            if x == 3 && y == 2 && world.a3[3][1] != "O " {
                if core.wd.contains(" 宿舍门禁卡 ") {
                    world.a3[3][1] = "O ".to_string();
                }
            }
            core.li = (core.li + 0.1).min(core.bli);
            if core.li <= 0.0 {
                print!("你体力不足\r\n");
                return 0;
            }
            coutt(core, &world.a3, x, y);
            if x == 3 && y == 2 && world.a3[3][1] != "O " {
                print!("需要宿舍门禁卡\r\n");
            }
            an = false;
        }
        let keys = gamekit::drain_keys();
        move_player(
            core, world, 3, 3, &mut x, &mut y, &mut an, &mut c, &mut a, &keys,
        );
        if x == 3 && y == 6 {
            world.a3[x as usize][y as usize] = "O ".to_string();
            return 2;
        }
        if x == 3 && y == 1 {
            world.a3[x as usize][y as usize] = "O ".to_string();
            return 4;
        }
        gamekit::sleep_ms(80);
    }
}

fn f4(core: &mut Core, world: &mut World, o: i32) -> i32 {
    gamekit::sleep_ms(500);
    let mut x: i32;
    let mut y: i32;
    let mut a = "  ".to_string();
    if o == 3 {
        x = 22;
        y = 27;
        world.a4[x as usize][y as usize] = "我".to_string();
    } else if o == 5 {
        x = 24;
        y = 13;
        world.a4[x as usize][y as usize] = "我".to_string();
    } else if o == 6 {
        x = 16;
        y = 5;
        world.a4[x as usize][y as usize] = "我".to_string();
    } else if o == 4 {
        // 原版未定义行为，实际不可达；定义落点
        x = 16;
        y = 5;
        world.a4[x as usize][y as usize] = "我".to_string();
    } else {
        // o == 0：回档后由 cd_o 机制恢复位置
        x = 0;
        y = 0;
    }
    let mut an = true;
    let mut c = core.way;
    loop {
        if c == 0 {
            // 体力变化
            if core.way == 1 {
                core.li = (core.li + 0.1).min(core.bli);
            } else if core.way == 2 {
                core.li = (core.li - 0.5).max(0.0);
            } else {
                core.li = (core.li - 1.0).max(0.0);
            }
            // NPC 移动
            for i in 0..10 {
                if i <= 1 {
                    // 保：警觉范围 5，每回合走两步
                    for _ in 0..2 {
                        let npc = world.a4npc[i].clone();
                        let (nx, ny) = (npc.x, npc.y);
                        let dx = npc.x - x;
                        let dy = npc.y - y;
                        let sx = if dx != 0 { dx / dx.abs() } else { 0 };
                        let sy = if dy != 0 { dy / dy.abs() } else { 0 };
                        if (npc.x - x).abs() + (npc.y - y).abs() - 1 <= 5 {
                            if (npc.x - x).abs() >= (npc.y - y).abs()
                                && world.a4[(nx - sx) as usize][ny as usize] == "  "
                                && npc.x != x
                            {
                                if world.a4[nx as usize][ny as usize] == npc.s {
                                    world.a4[nx as usize][ny as usize] = "  ".to_string();
                                }
                                world.a4npc[i].x -= sx;
                                let npc2 = world.a4npc[i].clone();
                                world.a4[npc2.x as usize][npc2.y as usize] = npc2.s.clone();
                                core.li -= 0.05;
                            } else if world.a4[nx as usize][(ny - sy) as usize] == "  "
                                && npc.y != y
                            {
                                if world.a4[nx as usize][ny as usize] == npc.s {
                                    world.a4[nx as usize][ny as usize] = "  ".to_string();
                                }
                                world.a4npc[i].y -= sy;
                                let npc2 = world.a4npc[i].clone();
                                world.a4[npc2.x as usize][npc2.y as usize] = npc2.s.clone();
                                core.li -= 0.05;
                            } else if world.a4[(nx - sx) as usize][ny as usize] == "  "
                                && npc.x != x
                            {
                                if world.a4[nx as usize][ny as usize] == npc.s {
                                    world.a4[nx as usize][ny as usize] = "  ".to_string();
                                }
                                world.a4npc[i].x -= sx;
                                let npc2 = world.a4npc[i].clone();
                                world.a4[npc2.x as usize][npc2.y as usize] = npc2.s.clone();
                                core.li -= 0.05;
                            }
                        }
                        let npc_now = world.a4npc[i].clone();
                        if (npc_now.x - x).abs() <= 1 && (npc_now.y - y).abs() <= 1 {
                            coutt(core, &world.a4, x, y);
                            print!("失败原因：你被保抓住了\r\n");
                            return 0;
                        }
                    }
                } else if i == 9 && core.wd.contains(" 大门钥匙 ") {
                    // 校：有大门钥匙后才会追逐（原版无拾取大门钥匙的逻辑，此分支为死代码）
                    for _ in 0..2 {
                        let npc = world.a4npc[i].clone();
                        let (nx, ny) = (npc.x, npc.y);
                        let dx = npc.x - x;
                        let dy = npc.y - y;
                        let sx = if dx != 0 { dx / dx.abs() } else { 0 };
                        let sy = if dy != 0 { dy / dy.abs() } else { 0 };
                        if (npc.x - x).abs() >= (npc.y - y).abs()
                            && world.a4[(nx - sx) as usize][ny as usize] == "  "
                            && npc.x != x
                        {
                            if world.a4[nx as usize][ny as usize] == npc.s {
                                world.a4[nx as usize][ny as usize] = "  ".to_string();
                            }
                            world.a4npc[i].x -= sx;
                            let npc2 = world.a4npc[i].clone();
                            world.a4[npc2.x as usize][npc2.y as usize] = npc2.s.clone();
                            core.li -= 0.05;
                        } else if world.a4[nx as usize][(ny - sy) as usize] == "  " && npc.y != y {
                            if world.a4[nx as usize][ny as usize] == npc.s {
                                world.a4[nx as usize][ny as usize] = "  ".to_string();
                            }
                            world.a4npc[i].y -= sy;
                            let npc2 = world.a4npc[i].clone();
                            world.a4[npc2.x as usize][npc2.y as usize] = npc2.s.clone();
                            core.li -= 0.05;
                        } else if world.a4[(nx - sx) as usize][ny as usize] == "  " && npc.x != x {
                            if world.a4[nx as usize][ny as usize] == npc.s {
                                world.a4[nx as usize][ny as usize] = "  ".to_string();
                            }
                            world.a4npc[i].x -= sx;
                            let npc2 = world.a4npc[i].clone();
                            world.a4[npc2.x as usize][npc2.y as usize] = npc2.s.clone();
                            core.li -= 0.05;
                        }
                        let npc_now = world.a4npc[i].clone();
                        if (npc_now.x - x).abs() <= 1 && (npc_now.y - y).abs() <= 1 {
                            coutt(core, &world.a4, x, y);
                            print!("失败原因：你被校抓住了\r\n");
                            return 0;
                        }
                    }
                } else {
                    // 师：警觉范围 3
                    let npc = world.a4npc[i].clone();
                    let (nx, ny) = (npc.x, npc.y);
                    let dx = npc.x - x;
                    let dy = npc.y - y;
                    let sx = if dx != 0 { dx / dx.abs() } else { 0 };
                    let sy = if dy != 0 { dy / dy.abs() } else { 0 };
                    if (npc.x - x).abs() + (npc.y - y).abs() - 1 <= 3 {
                        if (npc.x - x).abs() >= (npc.y - y).abs()
                            && world.a4[(nx - sx) as usize][ny as usize] == "  "
                            && npc.x != x
                        {
                            if world.a4[nx as usize][ny as usize] == npc.s {
                                world.a4[nx as usize][ny as usize] = "  ".to_string();
                            }
                            world.a4npc[i].x -= sx;
                            let npc2 = world.a4npc[i].clone();
                            world.a4[npc2.x as usize][npc2.y as usize] = npc2.s.clone();
                            core.li -= 0.1;
                        } else if world.a4[nx as usize][(ny - sy) as usize] == "  " && npc.y != y {
                            if world.a4[nx as usize][ny as usize] == npc.s {
                                world.a4[nx as usize][ny as usize] = "  ".to_string();
                            }
                            world.a4npc[i].y -= sy;
                            let npc2 = world.a4npc[i].clone();
                            world.a4[npc2.x as usize][npc2.y as usize] = npc2.s.clone();
                            core.li -= 0.1;
                        } else if world.a4[(nx - sx) as usize][ny as usize] == "  " && npc.x != x {
                            if world.a4[nx as usize][ny as usize] == npc.s {
                                world.a4[nx as usize][ny as usize] = "  ".to_string();
                            }
                            world.a4npc[i].x -= sx;
                            let npc2 = world.a4npc[i].clone();
                            world.a4[npc2.x as usize][npc2.y as usize] = npc2.s.clone();
                            core.li -= 0.1;
                        }
                    }
                    let npc_now = world.a4npc[i].clone();
                    if (npc_now.x - x).abs() <= 1 && (npc_now.y - y).abs() <= 1 {
                        coutt(core, &world.a4, x, y);
                        print!("失败原因：你被师抓住了\r\n");
                        return 0;
                    }
                }
            }
            c = core.way;
        }
        if an {
            if core.li <= 0.0 {
                coutt(core, &world.a4, x, y);
                print!("你体力不足\r\n");
                return 0;
            }
            if x == 24 && y == 13 && world.a4[25][13] != "O " {
                if core.wd.contains(" 体育办公室钥匙 ") {
                    world.a4[25][13] = "O ".to_string();
                }
            }
            if x == 16 && y == 5 && world.a4[17][5] != "O " {
                if core.wd.contains(" 图书馆钥匙 ") {
                    world.a4[17][5] = "O ".to_string();
                }
            }
            coutt(core, &world.a4, x, y);
            if x == 24 && y == 13 && world.a4[25][13] != "O " {
                print!("需要体育办公室钥匙\r\n");
            }
            if x == 16 && y == 5 && world.a4[17][5] != "O " {
                print!("需要图书馆钥匙\r\n");
            }
            an = false;
            if c == core.way {
                print!("按数字键切换速度\r\n");
            }
        }
        let keys = gamekit::drain_keys();
        move_player(
            core, world, 4, 4, &mut x, &mut y, &mut an, &mut c, &mut a, &keys,
        );
        speed(core, &mut c, &keys);
        if x == 22 && y == 26 {
            world.a4[x as usize][y as usize] = "O ".to_string();
            return 3;
        }
        if x == 25 && y == 13 {
            world.a4[x as usize][y as usize] = "O ".to_string();
            return 5;
        }
        if x == 17 && y == 5 {
            world.a4[x as usize][y as usize] = "O ".to_string();
            return 6;
        }
        gamekit::sleep_ms(80);
    }
}

fn f5(core: &mut Core, world: &mut World, _o: i32) -> i32 {
    gamekit::sleep_ms(500);
    let mut x = 2;
    let mut y = 11;
    let mut a = "  ".to_string();
    world.a5[x as usize][y as usize] = "我".to_string();
    let mut an = true;
    let mut c = core.way;
    loop {
        if c == 0 {
            // 体力变化
            if core.way == 1 {
                core.li = (core.li + 0.1).min(core.bli);
            } else if core.way == 2 {
                core.li = (core.li - 0.5).max(0.0);
            } else {
                core.li = (core.li - 1.0).max(0.0);
            }
            // 师移动
            for i in 0..2 {
                let npc = world.a5npc[i].clone();
                let (nx, ny) = (npc.x, npc.y);
                if (npc.x - x).abs() + (npc.y - y).abs() - 1 <= 3 {
                    let dx = npc.x - x;
                    let dy = npc.y - y;
                    let sx = if dx != 0 { dx / dx.abs() } else { 0 };
                    let sy = if dy != 0 { dy / dy.abs() } else { 0 };
                    if (npc.x - x).abs() >= (npc.y - y).abs()
                        && world.a5[(nx - sx) as usize][ny as usize] == "  "
                        && npc.x != x
                    {
                        if world.a5[nx as usize][ny as usize] == npc.s {
                            world.a5[nx as usize][ny as usize] = "  ".to_string();
                        }
                        world.a5npc[i].x -= sx;
                        let npc2 = world.a5npc[i].clone();
                        world.a5[npc2.x as usize][npc2.y as usize] = npc2.s.clone();
                        core.li -= 0.1;
                    } else if world.a5[nx as usize][(ny - sy) as usize] == "  " && npc.y != y {
                        if world.a5[nx as usize][ny as usize] == npc.s {
                            world.a5[nx as usize][ny as usize] = "  ".to_string();
                        }
                        world.a5npc[i].y -= sy;
                        let npc2 = world.a5npc[i].clone();
                        world.a5[npc2.x as usize][npc2.y as usize] = npc2.s.clone();
                        core.li -= 0.1;
                    } else if world.a5[(nx - sx) as usize][ny as usize] == "  " && npc.x != x {
                        if world.a5[nx as usize][ny as usize] == npc.s {
                            world.a5[nx as usize][ny as usize] = "  ".to_string();
                        }
                        world.a5npc[i].x -= sx;
                        let npc2 = world.a5npc[i].clone();
                        world.a5[npc2.x as usize][npc2.y as usize] = npc2.s.clone();
                        core.li -= 0.1;
                    }
                }
                let npc = world.a5npc[i].clone();
                if (npc.x - x).abs() <= 1 && (npc.y - y).abs() <= 1 {
                    coutt(core, &world.a5, x, y);
                    print!("失败原因：你被师抓住了\r\n");
                    return 0;
                }
            }
            c = core.way;
        }
        if an {
            if core.li <= 0.0 {
                coutt(core, &world.a5, x, y);
                print!("你体力不足\r\n");
                return 0;
            }
            coutt(core, &world.a5, x, y);
            an = false;
            if x == 12 && y == 2 {
                core.wd += "图书馆钥匙 ";
                a = "无".to_string();
                print!("获得图书馆钥匙\r\n");
            }
            if x == 12 && y == 11 {
                a = "无".to_string();
                print!("获得3瓶饮料,体力上限+3\r\n");
                core.bli += 3.0;
                core.li += 3.0;
            }
            if c == core.way {
                print!("按数字键切换速度\r\n");
            }
        }
        let keys = gamekit::drain_keys();
        move_player(
            core, world, 5, 5, &mut x, &mut y, &mut an, &mut c, &mut a, &keys,
        );
        speed(core, &mut c, &keys);
        if x == 1 && y == 11 {
            world.a5[x as usize][y as usize] = "O ".to_string();
            return 4;
        }
        gamekit::sleep_ms(80);
    }
}

// ---------------------------------------------------------------------------
// 回放文件生成（跨平台 C++）
// ---------------------------------------------------------------------------

fn escape_cpp(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '\x1b' => out.push_str("\\x1b"),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

fn write_replay(core: &Core) {
    let mut out = String::new();
    out.push_str("#include <iostream>\r\n#include <thread>\r\n#include <chrono>\r\nusing namespace std;\r\nint main(){\r\n");
    for frame in &core.start_end {
        out.push_str("cout << \"");
        out.push_str(&escape_cpp(frame));
        out.push_str("\";\r\n");
        out.push_str("this_thread::sleep_for(chrono::milliseconds(200));\r\n");
    }
    out.push_str("return 0;\r\n}\r\n");
    let _ = std::fs::write("你的游戏过程.cpp", out);
    print!("已生成 你的游戏过程.cpp\r\n");
}

// ---------------------------------------------------------------------------
// 主流程
// ---------------------------------------------------------------------------

#[allow(unused_assignments)]
fn main() {
    gamekit::init();
    gamekit::set_title("逃离学校8.1");
    gamekit::hide_cursor();

    let startt = "(作者:zengkeman)\r\n更新内容：\r\n1.删除彩蛋\r\n键位说明：\r\n1.使用WSAD键移动\r\n2.按数字键123可以切换回合移动步数\r\n3.按Q存档，存档后可在失败时回档(新)\r\n游戏规则：\r\n1.获得门卡等物品逃出学校即为胜利\r\n2.靠近柜可以获得物品\r\n3.被宿,师,保,校捉到或体力耗尽则失败\r\n温馨提示：\r\n1.此游戏纯属娱乐，无不良引导\r\n2.开始游戏前请将输入法切换到英文\r\n按回车开始游戏";

    // 进入原始模式（需要终端环境）
    let _raw = RawMode::enter().expect("需要终端环境（TTY）才能运行本游戏，请直接在终端中运行");

    // 逐字显示开场信息，等待回车（同时可输入 pl 触发重启指令）
    let mut start = String::new();
    for ch in startt.chars() {
        print!("{}", ch);
        let _ = std::io::stdout().flush();
        gamekit::sleep_ms(10);
    }
    loop {
        match gamekit::read_char_or_enter() {
            Some(c) => {
                start.push(c);
                print!("{}", c);
                let _ = std::io::stdout().flush();
            }
            None => break,
        }
    }

    let mut core = Core {
        li: 1.0,
        bli: 1.0,
        way: 1,
        start_end: Vec::new(),
        wd: " ".to_string(),
        cd: Vec::new(),
        cd_o: false,
        cd_x: 0,
        cd_y: 0,
        cd_a: String::new(),
    };
    let mut world = World {
        a1: make_map(8, 8),
        a2: make_map(19, 13),
        a3: make_map(6, 8),
        a4: make_map(31, 31),
        a5: make_map(14, 13),
        a2npc: Vec::new(),
        a4npc: Vec::new(),
        a5npc: Vec::new(),
    };

    loop {
        ready(&mut world, &mut core);
        gamekit::hide_cursor();
        core.way = 1;
        core.start_end.clear();
        gamekit::clear_screen();

        let mut x = 1;
        let mut w = 1;
        let mut y = 1;

        // 回档
        if !core.cd.is_empty() {
            gamekit::clear_screen();
            print!("是否回档(请按Y(es)/N(o))\r\n");
            let choice = gamekit::read_yn();
            if choice == 'Y' {
                gamekit::clear_screen();
                for (i, save) in core.cd.iter().enumerate() {
                    print!("{}.{}\r\n", i + 1, save.name);
                }
                print!("请选择存档：");
                let _ = std::io::stdout().flush();
                let mut sel = String::new();
                loop {
                    match gamekit::read_digit() {
                        Some(d) => {
                            sel.push(char::from_digit(d, 10).unwrap());
                            print!("{}", d);
                            let _ = std::io::stdout().flush();
                        }
                        None => break,
                    }
                }
                if !sel.is_empty() {
                    if let Ok(n) = sel.parse::<usize>() {
                        if n >= 1 && n <= core.cd.len() {
                            let save = core.cd[n - 1].clone();
                            x = save.g;
                            w = 0;
                            // 原版 CD_o 机制：首次 move() 时恢复位置
                            core.cd_o = true;
                            core.cd_x = save.x;
                            core.cd_y = save.y;
                            core.cd_a = save.a.clone();
                            world.a1 = save.a1.clone();
                            world.a2 = save.a2.clone();
                            world.a3 = save.a3.clone();
                            world.a4 = save.a4.clone();
                            world.a5 = save.a5.clone();
                            core.wd = save.wd.clone();
                            world.a2npc = save.a2npc.clone();
                            world.a4npc = save.a4npc.clone();
                            world.a5npc = save.a5npc.clone();
                            core.li = save.li;
                            core.bli = save.bli;
                            core.start_end = save.start_end.clone();
                        }
                    }
                }
            }
        }

        // 游戏主循环
        loop {
            y = w;
            w = x;
            if x == 1 {
                x = f1(&mut core, &mut world, y);
            } else if x == 2 {
                x = f2(&mut core, &mut world, y);
            } else if x == 3 {
                x = f3(&mut core, &mut world, y);
            } else if x == 4 {
                x = f4(&mut core, &mut world, y);
            } else if x == 5 {
                x = f5(&mut core, &mut world, y);
            } else if x == 6 {
                print!("太棒了，游戏成功\r\n");
                break;
            } else if x == 0 {
                print!("很遗憾，游戏失败\r\n");
                break;
            }
        }

        if start == "pl" {
            print!("你触发了重启指令，3秒后重启\r\n");
            gamekit::sleep_ms(3000);
            continue;
        }

        print!("是否生成你的游戏过程(请按Y(es)/N(o))");
        let _ = std::io::stdout().flush();
        loop {
            match gamekit::read_key() {
                Some(gamekit::KeyCode::Char(c)) if c.to_ascii_lowercase() == 'y' => {
                    write_replay(&core);
                    core.start_end.clear();
                    break;
                }
                Some(gamekit::KeyCode::Char(c)) if c.to_ascii_lowercase() == 'n' => {
                    core.start_end.clear();
                    break;
                }
                _ => continue,
            }
        }
        gamekit::show_cursor();
    }
}
