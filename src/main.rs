use std::env;
use std::ffi::OsStr;
use std::io::{self, Stdout, Write};
use std::process;
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode},
    execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{
        self, disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};

const WIDTH: i16 = 112;
const HEIGHT: i16 = 34;
const PLAYER_Y: i16 = HEIGHT - 3;
const INVADER_ROWS: usize = 5;
const INVADER_COLS: usize = 11;
const INVADER_X_SPACING: i16 = 9;
const INVADER_Y_SPACING: i16 = 3;
const INVADER_START_X: i16 = 7;
const INVADER_START_Y: i16 = 4;
const BUNKER_Y: i16 = HEIGHT - 8;
const PLAYER_RESPAWN_SECONDS: f32 = 2.0;
const WAVE_CLEAR_PAUSE_SECONDS: f32 = 2.0;
const LAST_INVADER_TRAVERSE_SECONDS: f32 = 1.5;
const TICK: Duration = Duration::from_millis(33);
const HELP: &str = "\
Terminal Invaders

Usage:
  terminal-invaders [--help] [--version]

Controls:
  Enter          start / restart
  Left, Right    move
  Space          fire
  P              pause
  S              sound on/off
  Q, Esc         quit
";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Point {
    x: i16,
    y: i16,
}

#[derive(Clone, Debug)]
struct Invader {
    row: usize,
    col: usize,
    alive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShotOwner {
    Player,
    Invader,
}

#[derive(Clone, Debug)]
struct Shot {
    pos: Point,
    owner: ShotOwner,
}

#[derive(Clone, Debug)]
struct Bunker {
    origin: Point,
    cells: [[bool; BUNKER_WIDTH]; BUNKER_HEIGHT],
}

const BUNKER_WIDTH: usize = 9;
const BUNKER_HEIGHT: usize = 4;
const BUNKER_PATTERN: [&str; BUNKER_HEIGHT] = ["  █████  ", " ███████ ", "█████████", "███   ███"];

#[derive(Clone, Debug)]
struct Saucer {
    pos: Point,
    active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EffectKind {
    Explosion,
    BigExplosion,
}

#[derive(Clone, Debug)]
struct Effect {
    pos: Point,
    age: f32,
    duration: f32,
    kind: EffectKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sound {
    Shoot,
    Hit,
    BigHit,
    PlayerHit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Attract,
    Playing,
    WaveCleared,
    Paused,
    GameOver,
}

#[derive(Clone, Debug)]
struct Game {
    mode: Mode,
    score: u32,
    lives: u8,
    wave: u32,
    player_x: i16,
    invaders: Vec<Invader>,
    invader_offset: Point,
    invader_dir: i16,
    invader_frame: usize,
    invader_step_timer: f32,
    player_shot_timer: f32,
    invader_shot_timer: f32,
    player_shot: Option<Shot>,
    invader_shots: Vec<Shot>,
    bunkers: Vec<Bunker>,
    saucer: Saucer,
    saucer_timer: f32,
    saucer_cooldown: f32,
    effects: Vec<Effect>,
    sounds: Vec<Sound>,
    sound_enabled: bool,
    player_respawn_timer: f32,
    player_burn_pos: Option<Point>,
    wave_clear_timer: f32,
    rng: Lcg,
}

#[derive(Clone, Debug)]
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        (self.state >> 32) as u32
    }

    fn range(&mut self, end: usize) -> usize {
        if end == 0 {
            0
        } else {
            self.next_u32() as usize % end
        }
    }
}

impl Bunker {
    fn new(x: i16, y: i16) -> Self {
        let mut cells = [[false; BUNKER_WIDTH]; BUNKER_HEIGHT];
        for (row, pattern) in BUNKER_PATTERN.iter().enumerate() {
            for (col, ch) in pattern.chars().enumerate() {
                cells[row][col] = ch != ' ';
            }
        }

        Self {
            origin: Point { x, y },
            cells,
        }
    }

    fn hit(&mut self, point: Point) -> bool {
        let x = point.x - self.origin.x;
        let y = point.y - self.origin.y;
        if x < 0 || y < 0 || x as usize >= BUNKER_WIDTH || y as usize >= BUNKER_HEIGHT {
            return false;
        }

        let cell = &mut self.cells[y as usize][x as usize];
        let was_solid = *cell;
        *cell = false;
        was_solid
    }
}

impl Game {
    fn new() -> Self {
        let mut game = Self {
            mode: Mode::Attract,
            score: 0,
            lives: 3,
            wave: 1,
            player_x: WIDTH / 2,
            invaders: Vec::new(),
            invader_offset: Point { x: 0, y: 0 },
            invader_dir: 1,
            invader_frame: 0,
            invader_step_timer: 0.0,
            player_shot_timer: 0.0,
            invader_shot_timer: 0.0,
            player_shot: None,
            invader_shots: Vec::new(),
            bunkers: Vec::new(),
            saucer: Saucer {
                pos: Point { x: WIDTH, y: 2 },
                active: false,
            },
            saucer_timer: 0.0,
            saucer_cooldown: 18.0,
            effects: Vec::new(),
            sounds: Vec::new(),
            sound_enabled: true,
            player_respawn_timer: 0.0,
            player_burn_pos: None,
            wave_clear_timer: 0.0,
            rng: Lcg::new(0x5eed_1234_cafe_beef),
        };
        game.reset_playfield();
        game
    }

    fn start(&mut self) {
        self.mode = Mode::Playing;
        self.score = 0;
        self.lives = 3;
        self.wave = 1;
        self.player_x = WIDTH / 2;
        self.player_respawn_timer = 0.0;
        self.player_burn_pos = None;
        self.wave_clear_timer = 0.0;
        self.reset_playfield();
    }

    fn reset_playfield(&mut self) {
        self.invaders = (0..INVADER_ROWS)
            .flat_map(|row| {
                (0..INVADER_COLS).map(move |col| Invader {
                    row,
                    col,
                    alive: true,
                })
            })
            .collect();
        self.invader_offset = Point { x: 0, y: 0 };
        self.invader_dir = 1;
        self.invader_frame = 0;
        self.invader_step_timer = 0.0;
        self.player_shot_timer = 0.0;
        self.invader_shot_timer = 0.0;
        self.player_shot = None;
        self.invader_shots.clear();
        self.effects.clear();
        self.sounds.clear();
        self.player_respawn_timer = 0.0;
        self.player_burn_pos = None;
        self.wave_clear_timer = 0.0;
        self.bunkers = vec![
            Bunker::new(14, BUNKER_Y),
            Bunker::new(40, BUNKER_Y),
            Bunker::new(66, BUNKER_Y),
            Bunker::new(92, BUNKER_Y),
        ];
        self.saucer = Saucer {
            pos: Point {
                x: WIDTH - sprite_width(saucer_sprite()) - 1,
                y: 2,
            },
            active: false,
        };
        self.saucer_timer = 0.0;
        self.saucer_cooldown = 12.0 + (self.wave as f32 * 1.5);
    }

    fn handle_key(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => return true,
            KeyCode::Enter if matches!(self.mode, Mode::Attract | Mode::GameOver) => self.start(),
            KeyCode::Char('p') | KeyCode::Char('P') => {
                self.mode = match self.mode {
                    Mode::Playing => Mode::Paused,
                    Mode::Paused => Mode::Playing,
                    other => other,
                };
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                self.sound_enabled = !self.sound_enabled;
                self.sounds.clear();
            }
            KeyCode::Left if self.mode == Mode::Playing && !self.player_is_respawning() => {
                self.player_x = (self.player_x - 2).max(player_min_x());
            }
            KeyCode::Right if self.mode == Mode::Playing && !self.player_is_respawning() => {
                self.player_x = (self.player_x + 2).min(player_max_x());
            }
            KeyCode::Char(' ') if self.mode == Mode::Playing && !self.player_is_respawning() => {
                self.fire_player_shot()
            }
            _ => {}
        }
        false
    }

    fn fire_player_shot(&mut self) {
        if self.player_shot.is_none() && !self.player_is_respawning() {
            self.player_shot = Some(Shot {
                pos: Point {
                    x: self.player_x,
                    y: player_origin().y - 1,
                },
                owner: ShotOwner::Player,
            });
            self.queue_sound(Sound::Shoot);
        }
    }

    fn update(&mut self, dt: f32) {
        self.update_effects(dt);

        if self.mode == Mode::WaveCleared {
            self.update_wave_clear(dt);
            return;
        }

        if self.mode != Mode::Playing {
            return;
        }

        if self.player_is_respawning() {
            self.update_player_respawn(dt);
            return;
        }

        self.update_saucer(dt);
        self.update_invaders(dt);
        self.damage_bunkers_touched_by_invaders();
        self.update_shots(dt);
        self.maybe_fire_invader_shot(dt);
        self.check_wave_or_loss();
    }

    fn update_saucer(&mut self, dt: f32) {
        if self.saucer.active {
            self.saucer_timer += dt;
            if self.saucer_timer >= 0.12 {
                self.saucer_timer = 0.0;
                self.saucer.pos.x -= 1;
                if self.saucer.pos.x < -sprite_width(saucer_sprite()) {
                    self.saucer.active = false;
                    self.saucer_cooldown = 18.0 + self.rng.range(9) as f32;
                }
            }
            return;
        }

        self.saucer_cooldown -= dt;
        if self.saucer_cooldown <= 0.0 {
            self.saucer.active = true;
            self.saucer.pos = Point {
                x: WIDTH - sprite_width(saucer_sprite()) - 1,
                y: 2,
            };
            self.saucer_timer = 0.0;
        }
    }

    fn update_invaders(&mut self, dt: f32) {
        self.invader_step_timer += dt;
        let speed = self.invader_step_interval();

        while self.invader_step_timer >= speed {
            self.invader_step_timer -= speed;
            self.step_invader_formation();
        }
    }

    fn step_invader_formation(&mut self) {
        let (left, right, _) = self.invader_bounds();
        if right + self.invader_dir >= WIDTH - 2 || left + self.invader_dir <= 1 {
            self.invader_dir *= -1;
            self.invader_offset.y += 1;
        } else {
            self.invader_offset.x += self.invader_dir;
        }
        self.invader_frame = 1 - self.invader_frame;
    }

    fn invader_step_interval(&self) -> f32 {
        let full_count = self.invaders.len().max(1) as f32;
        let alive_count = self.alive_invader_count().max(1) as f32;
        let killed_ratio = if full_count <= 1.0 {
            1.0
        } else {
            (full_count - alive_count) / (full_count - 1.0)
        };
        let base_speed = self.base_invader_step_interval();
        let last_speed = self.last_invader_step_interval();

        base_speed + (last_speed - base_speed) * killed_ratio
    }

    fn base_invader_step_interval(&self) -> f32 {
        (0.58 - self.wave as f32 * 0.035).max(0.08)
    }

    fn last_invader_step_interval(&self) -> f32 {
        let travel_steps = (WIDTH - 2 - max_invader_sprite_width()).max(1) as f32;
        LAST_INVADER_TRAVERSE_SECONDS / travel_steps
    }

    fn alive_invader_count(&self) -> usize {
        self.invaders.iter().filter(|invader| invader.alive).count()
    }

    fn update_shots(&mut self, dt: f32) {
        self.player_shot_timer += dt;
        while self.player_shot_timer >= 0.035 {
            self.player_shot_timer -= 0.035;
            self.step_player_shot();
        }

        self.invader_shot_timer += dt;
        while self.invader_shot_timer >= 0.065 {
            self.invader_shot_timer -= 0.065;
            self.step_invader_shots();
        }
    }

    fn step_player_shot(&mut self) {
        let Some(mut shot) = self.player_shot.take() else {
            return;
        };
        shot.pos.y -= 1;

        if shot.pos.y <= 0 {
            return;
        }

        if self.damage_bunker(shot.pos) {
            return;
        }

        if self.saucer.active && point_hits_sprite(shot.pos, self.saucer.pos, saucer_sprite()) {
            let pos = self.saucer.pos;
            self.saucer.active = false;
            self.score += 150 + (self.rng.range(4) as u32 * 50);
            self.spawn_effect(pos, EffectKind::BigExplosion);
            self.queue_sound(Sound::BigHit);
            return;
        }

        for index in 0..self.invaders.len() {
            if !self.invaders[index].alive {
                continue;
            }
            let pos = self.invader_position(&self.invaders[index]);
            if point_hits_sprite(
                shot.pos,
                pos,
                invader_sprite(self.invaders[index].row, self.invader_frame),
            ) {
                let hit_pos = pos;
                self.invaders[index].alive = false;
                self.score += match self.invaders[index].row {
                    0 => 40,
                    1 | 2 => 20,
                    _ => 10,
                };
                self.spawn_effect(hit_pos, EffectKind::Explosion);
                self.queue_sound(Sound::Hit);
                return;
            }
        }

        self.player_shot = Some(shot);
    }

    fn step_invader_shots(&mut self) {
        let mut shots = std::mem::take(&mut self.invader_shots);
        for mut shot in shots.drain(..) {
            shot.pos.y += 1;
            if shot.pos.y >= HEIGHT - 1 {
                continue;
            }
            if self.damage_bunker(shot.pos) {
                continue;
            }
            let player_pos = self.player_position();
            if point_hits_sprite(shot.pos, player_pos, player_sprite()) {
                self.player_hit();
                continue;
            }
            self.invader_shots.push(shot);
        }
    }

    fn damage_bunker(&mut self, point: Point) -> bool {
        for bunker in &mut self.bunkers {
            if bunker.hit(point) {
                return true;
            }
        }
        false
    }

    fn damage_bunkers_touched_by_invaders(&mut self) {
        let contacts: Vec<Point> = self
            .invaders
            .iter()
            .filter(|invader| invader.alive)
            .flat_map(|invader| {
                let origin = self.invader_position(invader);
                sprite_solid_points(origin, invader_sprite(invader.row, self.invader_frame))
            })
            .filter(|point| point.y >= BUNKER_Y && point.y < BUNKER_Y + BUNKER_HEIGHT as i16)
            .collect();

        for point in contacts {
            self.damage_bunker(point);
        }
    }

    fn maybe_fire_invader_shot(&mut self, dt: f32) {
        let chance = (dt * (1.1 + self.wave as f32 * 0.22) * 1000.0) as usize;
        if self.invader_shots.len() >= 4 || self.rng.range(1000) >= chance {
            return;
        }

        let shooters = self.bottom_alive_invaders();
        if shooters.is_empty() {
            return;
        }
        let invader = shooters[self.rng.range(shooters.len())];
        let pos = self.invader_position(&self.invaders[invader]);
        let sprite = invader_sprite(self.invaders[invader].row, self.invader_frame);
        self.invader_shots.push(Shot {
            pos: Point {
                x: pos.x + sprite_width(sprite) / 2,
                y: pos.y + sprite_height(sprite),
            },
            owner: ShotOwner::Invader,
        });
    }

    fn bottom_alive_invaders(&self) -> Vec<usize> {
        let mut shooters = Vec::new();
        for col in 0..INVADER_COLS {
            if let Some((idx, _)) = self
                .invaders
                .iter()
                .enumerate()
                .filter(|(_, invader)| invader.alive && invader.col == col)
                .max_by_key(|(_, invader)| invader.row)
            {
                shooters.push(idx);
            }
        }
        shooters
    }

    fn player_hit(&mut self) {
        if self.player_is_respawning() || self.mode == Mode::GameOver {
            return;
        }

        let burn_pos = self.player_position();
        self.lives = self.lives.saturating_sub(1);
        self.player_shot = None;
        self.invader_shots.clear();
        self.player_respawn_timer = PLAYER_RESPAWN_SECONDS;
        self.player_burn_pos = Some(burn_pos);
        self.spawn_effect(burn_pos, EffectKind::BigExplosion);
        self.queue_sound(Sound::PlayerHit);
    }

    fn check_wave_or_loss(&mut self) {
        if self.invaders.iter().all(|invader| !invader.alive) {
            self.wave += 1;
            self.mode = Mode::WaveCleared;
            self.wave_clear_timer = WAVE_CLEAR_PAUSE_SECONDS;
            self.player_shot = None;
            self.invader_shots.clear();
            self.saucer.active = false;
            return;
        }

        let (_, _, bottom) = self.invader_bounds();
        if bottom >= BUNKER_Y + BUNKER_HEIGHT as i16 || bottom >= PLAYER_Y {
            self.mode = Mode::GameOver;
        }
    }

    fn update_wave_clear(&mut self, dt: f32) {
        self.wave_clear_timer = (self.wave_clear_timer - dt).max(0.0);
        if self.wave_clear_timer == 0.0 {
            self.reset_playfield();
            self.mode = Mode::Playing;
        }
    }

    fn invader_position(&self, invader: &Invader) -> Point {
        Point {
            x: INVADER_START_X + invader.col as i16 * INVADER_X_SPACING + self.invader_offset.x,
            y: INVADER_START_Y + invader.row as i16 * INVADER_Y_SPACING + self.invader_offset.y,
        }
    }

    fn invader_bounds(&self) -> (i16, i16, i16) {
        let mut left = WIDTH;
        let mut right = 0;
        let mut bottom = 0;
        for invader in self.invaders.iter().filter(|invader| invader.alive) {
            let pos = self.invader_position(invader);
            let sprite = invader_sprite(invader.row, self.invader_frame);
            left = left.min(pos.x);
            right = right.max(pos.x + sprite_width(sprite) - 1);
            bottom = bottom.max(pos.y + sprite_height(sprite) - 1);
        }
        (left, right, bottom)
    }

    fn update_effects(&mut self, dt: f32) {
        for effect in &mut self.effects {
            effect.age += dt;
        }
        self.effects.retain(|effect| effect.age < effect.duration);
    }

    fn update_player_respawn(&mut self, dt: f32) {
        self.player_respawn_timer = (self.player_respawn_timer - dt).max(0.0);
        if self.player_respawn_timer == 0.0 {
            self.player_burn_pos = None;
            if self.lives == 0 {
                self.mode = Mode::GameOver;
            } else {
                self.player_x = WIDTH / 2;
            }
        }
    }

    fn player_is_respawning(&self) -> bool {
        self.player_respawn_timer > 0.0
    }

    fn spawn_effect(&mut self, pos: Point, kind: EffectKind) {
        let duration = match kind {
            EffectKind::Explosion => 0.42,
            EffectKind::BigExplosion => 0.7,
        };
        self.effects.push(Effect {
            pos,
            age: 0.0,
            duration,
            kind,
        });
    }

    fn queue_sound(&mut self, sound: Sound) {
        if self.sound_enabled {
            self.sounds.push(sound);
        }
    }

    fn drain_sounds(&mut self) -> Vec<Sound> {
        std::mem::take(&mut self.sounds)
    }

    fn player_position(&self) -> Point {
        Point {
            x: self.player_x - sprite_width(player_sprite()) / 2,
            y: player_origin().y,
        }
    }
}

fn point_hits_sprite(point: Point, origin: Point, sprite: &[&str]) -> bool {
    let x = point.x - origin.x;
    let y = point.y - origin.y;
    if x < 0 || y < 0 || y as usize >= sprite.len() {
        return false;
    }

    sprite[y as usize]
        .chars()
        .nth(x as usize)
        .is_some_and(|ch| ch != ' ')
}

fn sprite_solid_points(origin: Point, sprite: &[&str]) -> Vec<Point> {
    let mut points = Vec::new();
    for (y, line) in sprite.iter().enumerate() {
        for (x, ch) in line.chars().enumerate() {
            if ch != ' ' {
                points.push(Point {
                    x: origin.x + x as i16,
                    y: origin.y + y as i16,
                });
            }
        }
    }
    points
}

fn sprite_width(sprite: &[&str]) -> i16 {
    sprite
        .iter()
        .map(|line| line.chars().count() as i16)
        .max()
        .unwrap_or(0)
}

fn sprite_height(sprite: &[&str]) -> i16 {
    sprite.len() as i16
}

fn max_invader_sprite_width() -> i16 {
    (0..INVADER_ROWS)
        .flat_map(|row| [invader_sprite(row, 0), invader_sprite(row, 1)])
        .map(sprite_width)
        .max()
        .unwrap_or(1)
}

fn invader_sprite(row: usize, frame: usize) -> &'static [&'static str] {
    match (row, frame % 2) {
        (0, 0) => &[" ╱█╲ ", "╱███╲"],
        (0, _) => &["╲███╱", " ╲█╱ "],
        (1 | 2, 0) => &["▄█▄█▄", "╰███╯"],
        (1 | 2, _) => &["╭███╮", "▀█▀█▀"],
        (_, 0) => &["▟███▙", "▜█ █▛"],
        (_, _) => &["▙███▟", "▛█ █▜"],
    }
}

fn saucer_sprite() -> &'static [&'static str] {
    &[" ╭═O═╮ ", "╰═███═╯"]
}

fn player_sprite() -> &'static [&'static str] {
    &["   ▲   ", "███████"]
}

fn player_origin() -> Point {
    Point {
        x: 0,
        y: PLAYER_Y - sprite_height(player_sprite()) + 1,
    }
}

fn player_min_x() -> i16 {
    1 + sprite_width(player_sprite()) / 2
}

fn player_max_x() -> i16 {
    WIDTH - 2 - sprite_width(player_sprite()) / 2
}

fn player_fire_sprite(frame: usize) -> &'static [&'static str] {
    match frame % 2 {
        0 => &["  ▲▲▲  ", "███▓███"],
        _ => &[" ▲▲▲▲▲ ", "███████"],
    }
}

fn explosion_sprite(effect: &Effect) -> &'static [&'static str] {
    let progress = effect.age / effect.duration;
    match (effect.kind, progress < 0.45) {
        (EffectKind::Explosion, true) => &[" \\|/ ", "-███-", " /|\\ "],
        (EffectKind::Explosion, false) => &["  *  ", " *** ", "  *  "],
        (EffectKind::BigExplosion, true) => &[" \\|||/ ", "-█████-", " /|||\\ "],
        (EffectKind::BigExplosion, false) => &["  \\|/  ", "--***--", "  /|\\  "],
    }
}

fn main() -> io::Result<()> {
    match handle_cli_args() {
        CliAction::Run => {}
        CliAction::ExitOk => return Ok(()),
        CliAction::ExitErr => process::exit(2),
    }

    let mut terminal = Terminal::enter()?;
    let mut game = Game::new();
    let mut last_tick = Instant::now();

    loop {
        while event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                if game.handle_key(key.code) {
                    return Ok(());
                }
            }
        }

        let now = Instant::now();
        let dt = now.duration_since(last_tick);
        if dt >= TICK {
            last_tick = now;
            game.update(dt.as_secs_f32());
            terminal.play_sounds(game.drain_sounds())?;
            terminal.draw(&game)?;
        }

        std::thread::sleep(Duration::from_millis(2));
    }
}

enum CliAction {
    Run,
    ExitOk,
    ExitErr,
}

fn handle_cli_args() -> CliAction {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.is_empty() {
        return CliAction::Run;
    }

    if args.len() == 1 {
        let arg = &args[0];
        if arg == OsStr::new("--version") || arg == OsStr::new("-V") {
            println!("terminal-invaders {}", env!("CARGO_PKG_VERSION"));
            return CliAction::ExitOk;
        }

        if arg == OsStr::new("--help") || arg == OsStr::new("-h") {
            print!("{HELP}");
            return CliAction::ExitOk;
        }
    }

    eprintln!("unknown argument. Try `terminal-invaders --help`.");
    CliAction::ExitErr
}

struct Terminal {
    stdout: Stdout,
}

impl Terminal {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Hide)?;
        Ok(Self { stdout })
    }

    fn draw(&mut self, game: &Game) -> io::Result<()> {
        let (term_width, term_height) = terminal::size()?;
        queue!(self.stdout, MoveTo(0, 0), Clear(ClearType::All))?;

        if term_width < WIDTH as u16 || term_height < HEIGHT as u16 {
            queue!(
                self.stdout,
                SetForegroundColor(Color::Red),
                Print(format!(
                    "Terminal too small. Need at least {}x{}, current size is {}x{}.",
                    WIDTH, HEIGHT, term_width, term_height
                )),
                ResetColor
            )?;
            self.stdout.flush()?;
            return Ok(());
        }

        let origin = render_origin(term_width, term_height);
        self.draw_hud(game, origin)?;
        self.draw_border(game, origin)?;
        self.draw_bunkers(game, origin)?;
        self.draw_invaders(game, origin)?;
        self.draw_saucer(game, origin)?;
        self.draw_effects(game, origin)?;
        self.draw_shots(game, origin)?;
        self.draw_player(game, origin)?;
        self.draw_overlay(game, origin)?;
        self.stdout.flush()
    }

    fn draw_hud(&mut self, game: &Game, origin: Point) -> io::Result<()> {
        queue!(
            self.stdout,
            screen_pos(origin, 1, 0),
            SetForegroundColor(Color::White),
            Print(format!(
                "Score {:05}   Lives {}   Wave {}   Sound {}   S sound   Q/Esc quit   P pause",
                game.score,
                game.lives,
                game.wave,
                if game.sound_enabled { "on" } else { "off" }
            )),
            ResetColor
        )
    }

    fn draw_border(&mut self, game: &Game, origin: Point) -> io::Result<()> {
        queue!(
            self.stdout,
            SetForegroundColor(tinted(game, Color::DarkGrey))
        )?;
        for x in 0..WIDTH {
            queue!(
                self.stdout,
                screen_pos(origin, x, 1),
                Print("─"),
                screen_pos(origin, x, HEIGHT - 1),
                Print("─")
            )?;
        }
        for y in 1..HEIGHT {
            queue!(
                self.stdout,
                screen_pos(origin, 0, y),
                Print("│"),
                screen_pos(origin, WIDTH - 1, y),
                Print("│")
            )?;
        }
        queue!(
            self.stdout,
            screen_pos(origin, 0, 1),
            Print("┌"),
            screen_pos(origin, WIDTH - 1, 1),
            Print("┐"),
            screen_pos(origin, 0, HEIGHT - 1),
            Print("└"),
            screen_pos(origin, WIDTH - 1, HEIGHT - 1),
            Print("┘"),
            ResetColor
        )
    }

    fn draw_bunkers(&mut self, game: &Game, origin: Point) -> io::Result<()> {
        queue!(self.stdout, SetForegroundColor(tinted(game, Color::Green)))?;
        for bunker in &game.bunkers {
            for y in 0..BUNKER_HEIGHT {
                for x in 0..BUNKER_WIDTH {
                    if bunker.cells[y][x] {
                        queue!(
                            self.stdout,
                            screen_pos(
                                origin,
                                bunker.origin.x + x as i16,
                                bunker.origin.y + y as i16
                            ),
                            Print("█")
                        )?;
                    }
                }
            }
        }
        queue!(self.stdout, ResetColor)
    }

    fn draw_invaders(&mut self, game: &Game, origin: Point) -> io::Result<()> {
        for invader in game.invaders.iter().filter(|invader| invader.alive) {
            let pos = game.invader_position(invader);
            let color = match invader.row {
                0 => Color::Magenta,
                1 | 2 => Color::Cyan,
                _ => Color::Yellow,
            };
            draw_sprite(
                &mut self.stdout,
                origin,
                pos,
                invader_sprite(invader.row, game.invader_frame),
                tinted(game, color),
            )?;
        }
        Ok(())
    }

    fn draw_saucer(&mut self, game: &Game, origin: Point) -> io::Result<()> {
        if !game.saucer.active || game.saucer.pos.x <= 0 {
            return Ok(());
        }
        draw_sprite(
            &mut self.stdout,
            origin,
            game.saucer.pos,
            saucer_sprite(),
            tinted(game, Color::Red),
        )
    }

    fn draw_shots(&mut self, game: &Game, origin: Point) -> io::Result<()> {
        if let Some(shot) = &game.player_shot {
            draw_shot(&mut self.stdout, origin, shot, game)?;
        }
        for shot in &game.invader_shots {
            draw_shot(&mut self.stdout, origin, shot, game)?;
        }
        Ok(())
    }

    fn draw_effects(&mut self, game: &Game, origin: Point) -> io::Result<()> {
        for effect in &game.effects {
            let color = match effect.kind {
                EffectKind::Explosion => Color::Yellow,
                EffectKind::BigExplosion => Color::Red,
            };
            draw_sprite(
                &mut self.stdout,
                origin,
                effect.pos,
                explosion_sprite(effect),
                tinted(game, color),
            )?;
        }
        Ok(())
    }

    fn draw_player(&mut self, game: &Game, origin: Point) -> io::Result<()> {
        if game.mode == Mode::GameOver {
            return Ok(());
        }
        if game.player_is_respawning() {
            let frame = (game.player_respawn_timer * 8.0) as usize;
            return draw_sprite(
                &mut self.stdout,
                origin,
                game.player_burn_pos
                    .unwrap_or_else(|| game.player_position()),
                player_fire_sprite(frame),
                Color::Red,
            );
        }
        draw_sprite(
            &mut self.stdout,
            origin,
            game.player_position(),
            player_sprite(),
            tinted(game, Color::Blue),
        )
    }

    fn draw_overlay(&mut self, game: &Game, origin: Point) -> io::Result<()> {
        let lines: Vec<String> = match game.mode {
            Mode::Attract => vec![
                "TERMINAL INVADERS".to_string(),
                String::new(),
                "Press Enter to start".to_string(),
                "Arrow keys move   Space fires".to_string(),
            ],
            Mode::WaveCleared => vec![
                "WAVE CLEARED".to_string(),
                String::new(),
                format!("Prepare for Wave {}", game.wave),
            ],
            Mode::Paused => vec![
                "PAUSED".to_string(),
                String::new(),
                "Press P to resume".to_string(),
            ],
            Mode::GameOver => vec![
                "GAME OVER".to_string(),
                String::new(),
                "Press Enter to play again".to_string(),
            ],
            Mode::Playing => return Ok(()),
        };

        let start_y = HEIGHT / 2 - lines.len() as i16 / 2;
        queue!(self.stdout, SetForegroundColor(Color::White))?;
        for (idx, line) in lines.iter().enumerate() {
            let x = (WIDTH - line.chars().count() as i16) / 2;
            queue!(
                self.stdout,
                screen_pos(origin, x, start_y + idx as i16),
                Print(line)
            )?;
        }
        queue!(self.stdout, ResetColor)
    }

    fn play_sounds(&mut self, sounds: Vec<Sound>) -> io::Result<()> {
        for sound in sounds {
            let bells = match sound {
                Sound::Shoot => 1,
                Sound::Hit => 1,
                Sound::BigHit => 2,
                Sound::PlayerHit => 3,
            };
            for _ in 0..bells {
                write!(self.stdout, "\x07")?;
            }
        }
        self.stdout.flush()
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = execute!(self.stdout, Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

fn draw_shot(stdout: &mut Stdout, origin: Point, shot: &Shot, game: &Game) -> io::Result<()> {
    let (glyph, color) = match shot.owner {
        ShotOwner::Player => ("│", Color::White),
        ShotOwner::Invader => ("╵", Color::Red),
    };
    queue!(
        stdout,
        SetForegroundColor(tinted(game, color)),
        screen_pos(origin, shot.pos.x, shot.pos.y),
        Print(glyph),
        ResetColor
    )
}

fn tinted(game: &Game, color: Color) -> Color {
    if game.player_is_respawning() {
        Color::Red
    } else {
        color
    }
}

fn draw_sprite(
    stdout: &mut Stdout,
    viewport: Point,
    sprite_origin: Point,
    sprite: &[&str],
    color: Color,
) -> io::Result<()> {
    queue!(stdout, SetForegroundColor(color))?;
    for (row, line) in sprite.iter().enumerate() {
        let y = sprite_origin.y + row as i16;
        if sprite_origin.x < 0 || y < 0 {
            continue;
        }
        queue!(
            stdout,
            screen_pos(viewport, sprite_origin.x, y),
            Print(*line)
        )?;
    }
    queue!(stdout, ResetColor)
}

fn render_origin(term_width: u16, term_height: u16) -> Point {
    Point {
        x: ((term_width as i16 - WIDTH) / 2).max(0),
        y: ((term_height as i16 - HEIGHT) / 2).max(0),
    }
}

fn screen_pos(origin: Point, x: i16, y: i16) -> MoveTo {
    MoveTo((origin.x + x) as u16, (origin.y + y) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bunker_hit_removes_solid_cell_once() {
        let mut bunker = Bunker::new(10, 10);
        let point = Point { x: 12, y: 10 };

        assert!(bunker.hit(point));
        assert!(!bunker.hit(point));
    }

    #[test]
    fn player_shot_scores_and_kills_invader() {
        let mut game = Game::new();
        game.mode = Mode::Playing;
        let target = game.invader_position(&game.invaders[0]);
        game.player_shot = Some(Shot {
            pos: Point {
                x: target.x + 2,
                y: target.y + 1,
            },
            owner: ShotOwner::Player,
        });

        game.step_player_shot();

        assert!(!game.invaders[0].alive);
        assert_eq!(game.score, 40);
        assert!(game.player_shot.is_none());
        assert_eq!(game.effects.len(), 1);
        assert_eq!(game.sounds, vec![Sound::Hit]);
    }

    #[test]
    fn clearing_wave_advances_to_next_wave() {
        let mut game = Game::new();
        game.mode = Mode::Playing;
        for invader in &mut game.invaders {
            invader.alive = false;
        }

        game.check_wave_or_loss();

        assert_eq!(game.wave, 2);
        assert_eq!(game.mode, Mode::WaveCleared);
        assert_eq!(game.wave_clear_timer, WAVE_CLEAR_PAUSE_SECONDS);
        assert!(game.invaders.iter().all(|invader| !invader.alive));

        game.update(WAVE_CLEAR_PAUSE_SECONDS + 0.1);

        assert_eq!(game.mode, Mode::Playing);
        assert!(game.invaders.iter().all(|invader| invader.alive));
    }

    #[test]
    fn invaders_destroy_bunker_cells_they_touch() {
        let mut game = Game::new();
        game.mode = Mode::Playing;
        for invader in &mut game.invaders {
            invader.alive = false;
        }
        game.invaders[0].alive = true;
        game.invaders[0].row = 0;
        game.invaders[0].col = 0;
        game.invader_offset = Point {
            x: game.bunkers[0].origin.x - INVADER_START_X,
            y: game.bunkers[0].origin.y - INVADER_START_Y,
        };
        let solid_before = game.bunkers[0]
            .cells
            .iter()
            .flatten()
            .filter(|cell| **cell)
            .count();

        game.damage_bunkers_touched_by_invaders();

        let solid_after = game.bunkers[0]
            .cells
            .iter()
            .flatten()
            .filter(|cell| **cell)
            .count();
        assert!(solid_after < solid_before);
    }

    #[test]
    fn player_hit_reduces_lives_and_eventually_ends_game() {
        let mut game = Game::new();
        game.mode = Mode::Playing;

        game.player_hit();
        game.update(PLAYER_RESPAWN_SECONDS + 0.1);
        game.player_hit();
        game.update(PLAYER_RESPAWN_SECONDS + 0.1);
        game.player_hit();

        assert_eq!(game.lives, 0);
        assert_eq!(game.mode, Mode::Playing);
        assert!(game.player_is_respawning());

        game.update(PLAYER_RESPAWN_SECONDS + 0.1);

        assert_eq!(game.mode, Mode::GameOver);
    }

    #[test]
    fn player_hit_queues_fire_delay_effect_and_sound() {
        let mut game = Game::new();
        game.mode = Mode::Playing;

        game.player_hit();

        assert_eq!(game.lives, 2);
        assert!(game.player_is_respawning());
        assert_eq!(game.effects.len(), 1);
        assert_eq!(game.sounds, vec![Sound::PlayerHit]);
    }

    #[test]
    fn sound_toggle_suppresses_new_sounds() {
        let mut game = Game::new();
        game.mode = Mode::Playing;

        game.handle_key(KeyCode::Char('s'));
        game.fire_player_shot();

        assert!(!game.sound_enabled);
        assert!(game.sounds.is_empty());
    }

    #[test]
    fn render_origin_centers_game_in_larger_terminal() {
        assert_eq!(
            render_origin((WIDTH + 20) as u16, (HEIGHT + 8) as u16),
            Point { x: 10, y: 4 }
        );
    }

    #[test]
    fn player_burns_where_hit_until_respawn_finishes() {
        let mut game = Game::new();
        game.mode = Mode::Playing;
        game.player_x = player_min_x();
        let hit_pos = game.player_position();

        game.player_hit();
        game.update(PLAYER_RESPAWN_SECONDS / 2.0);

        assert_eq!(game.player_burn_pos, Some(hit_pos));
        assert_eq!(game.player_x, player_min_x());

        game.update(PLAYER_RESPAWN_SECONDS);

        assert_eq!(game.player_burn_pos, None);
        assert_eq!(game.player_x, WIDTH / 2);
    }

    #[test]
    fn invaders_toggle_animation_frame_when_formation_steps() {
        let mut game = Game::new();
        game.mode = Mode::Playing;

        game.step_invader_formation();

        assert_eq!(game.invader_frame, 1);
    }

    #[test]
    fn invader_speed_reaches_full_width_in_one_and_half_seconds_for_last_alien() {
        let mut game = Game::new();
        for invader in &mut game.invaders {
            invader.alive = false;
        }
        game.invaders[0].alive = true;

        let travel_steps = (WIDTH - 2 - max_invader_sprite_width()).max(1) as f32;

        assert!((game.invader_step_interval() * travel_steps - 1.5).abs() < 0.0001);
    }

    #[test]
    fn invader_speed_interpolates_from_base_as_aliens_are_lost() {
        let mut game = Game::new();
        let full_speed = game.invader_step_interval();
        game.invaders[0].alive = false;
        let after_one_kill = game.invader_step_interval();

        assert_eq!(full_speed, game.base_invader_step_interval());
        assert!(after_one_kill < full_speed);
        assert!(after_one_kill > game.last_invader_step_interval());
    }
}
