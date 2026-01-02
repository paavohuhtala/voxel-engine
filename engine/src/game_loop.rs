use std::time::Instant;

// Based on code from https://github.com/tuzz/game-loop
// Which is based on the classic article https://gafferongames.com/post/fix_your_timestep/

pub trait Game {
    fn update(&mut self, time: &GameLoopTime) -> anyhow::Result<()>;
    fn render(&mut self, _time: &GameLoopTime) -> anyhow::Result<()> {
        Ok(())
    }
}

pub struct GameLoopConfig {
    pub updates_per_s: u32,
    pub max_frame_time_s: f64,
}

pub struct GameLoop<G: Game> {
    pub game: G,
    pub exit_next_frame: bool,

    previous_instant: Instant,
    current_instant: Instant,
    accumulated_time_s: f64,
    fixed_time_step_s: f64,
    number_of_updates: u64,
    number_of_renders: u64,
    last_frame_time_s: f64,
    running_time_s: f64,
    blending_factor: f64,
    max_frame_time_s: f64,
}

impl<G: Game> GameLoop<G> {
    pub fn new(game: G, config: GameLoopConfig) -> Self {
        Self {
            game,
            exit_next_frame: false,

            previous_instant: Instant::now(),
            current_instant: Instant::now(),
            accumulated_time_s: 0.0,
            fixed_time_step_s: 1.0 / config.updates_per_s as f64,
            number_of_updates: 0,
            number_of_renders: 0,
            last_frame_time_s: 0.0,
            running_time_s: 0.0,
            blending_factor: 0.0,
            max_frame_time_s: config.max_frame_time_s,
        }
    }

    pub fn next_frame(&mut self) -> anyhow::Result<GameLoopResult> {
        if self.exit_next_frame {
            return Ok(GameLoopResult::Exit);
        }

        self.current_instant = Instant::now();

        let mut elapsed_s = self
            .current_instant
            .duration_since(self.previous_instant)
            .as_secs_f64();
        elapsed_s = elapsed_s.min(self.max_frame_time_s);

        self.last_frame_time_s = elapsed_s;
        self.running_time_s += elapsed_s;
        self.accumulated_time_s += elapsed_s;

        while self.accumulated_time_s >= self.fixed_time_step_s {
            let time = GameLoopTime {
                delta_time_s: self.fixed_time_step_s,
                elapsed_time_s: self.running_time_s,
                // Blending factor is only relevant for rendering
                blending_factor: 0.0,
            };
            self.game.update(&time)?;
            self.accumulated_time_s -= self.fixed_time_step_s;
            self.number_of_updates += 1;
        }

        self.blending_factor = self.accumulated_time_s / self.fixed_time_step_s;

        let time = GameLoopTime {
            delta_time_s: self.last_frame_time_s,
            elapsed_time_s: self.running_time_s,
            blending_factor: self.blending_factor,
        };
        self.game.render(&time)?;

        Ok(GameLoopResult::Continue)
    }

    pub fn exit(&mut self) {
        self.exit_next_frame = true;
    }

    pub fn last_frame_time_s(&self) -> f64 {
        self.last_frame_time_s
    }

    pub fn running_time_s(&self) -> f64 {
        self.running_time_s
    }

    pub fn blending_factor(&self) -> f64 {
        self.blending_factor
    }

    pub fn number_of_updates(&self) -> u64 {
        self.number_of_updates
    }

    pub fn number_of_renders(&self) -> u64 {
        self.number_of_renders
    }
}

pub enum GameLoopResult {
    Continue,
    Exit,
}

pub struct GameLoopTime {
    pub delta_time_s: f64,
    pub elapsed_time_s: f64,
    pub blending_factor: f64,
}
