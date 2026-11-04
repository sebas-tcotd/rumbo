//! Cronometraje: vueltas y checkpoints sobre la coordenada de distancia `s`.

use crate::hash::Fnv64;
use crate::track::{wrap_delta, CompiledTrack};

#[derive(Clone, Debug, Default)]
pub struct LapTimer {
    /// Ya se cruzó la meta al menos una vez (la vuelta de salida no cuenta).
    pub started: bool,
    /// Índice del próximo checkpoint esperado (1..len); `len` = toca la meta.
    pub next_cp: usize,
    pub lap_start_tick: u64,
    pub lap: u32,
    pub last_lap_ticks: Option<u64>,
    pub best_lap_ticks: Option<u64>,
    pub completed: Vec<u64>,
    /// Vuelta anulada (meta cruzada sin pasar por todos los checkpoints).
    pub invalidated: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LapEvent {
    None,
    Started,
    Checkpoint(usize),
    Completed { ticks: u64, best: bool },
    Invalidated,
}

impl LapTimer {
    /// Procesa el avance de `prev_s` a `cur_s` en el tick actual.
    pub fn update(&mut self, track: &CompiledTrack, prev_s: f32, cur_s: f32, tick: u64) -> LapEvent {
        if !track.closed {
            return self.update_sprint(track, prev_s, cur_s, tick);
        }
        let l = track.length;
        let ds = wrap_delta(prev_s, cur_s, l);
        if ds <= 0.0 || ds > 50.0 {
            return LapEvent::None; // hacia atrás o salto (reposicionamiento)
        }
        let crossed = |c: f32| {
            let d = (c - prev_s).rem_euclid(l);
            d > 0.0 && d <= ds
        };
        let cps = &track.checkpoints;
        let mut event = LapEvent::None;
        if self.started && self.next_cp < cps.len() && crossed(cps[self.next_cp]) {
            event = LapEvent::Checkpoint(self.next_cp);
            self.next_cp += 1;
        }
        if crossed(0.0) {
            if !self.started {
                self.started = true;
                event = LapEvent::Started;
            } else if self.next_cp >= cps.len() {
                let ticks = tick - self.lap_start_tick;
                let best = self.best_lap_ticks.is_none_or(|b| ticks < b);
                if best {
                    self.best_lap_ticks = Some(ticks);
                }
                self.last_lap_ticks = Some(ticks);
                self.completed.push(ticks);
                self.lap += 1;
                event = LapEvent::Completed { ticks, best };
            } else {
                self.invalidated += 1;
                event = LapEvent::Invalidated;
            }
            self.lap_start_tick = tick;
            self.next_cp = 1;
        }
        event
    }

    /// Sprint: el cronómetro corre de la línea de salida a la de llegada.
    /// Al llegar se detiene; para otro intento se vuelve a la salida (`FLAG_RESTART`).
    fn update_sprint(&mut self, track: &CompiledTrack, prev_s: f32, cur_s: f32, tick: u64) -> LapEvent {
        let ds = cur_s - prev_s;
        if ds <= 0.0 || ds > 50.0 {
            return LapEvent::None;
        }
        let crossed = |c: f32| prev_s < c && cur_s >= c;
        let cps = &track.checkpoints;
        let finish = cps.len() - 1;
        if !self.started {
            if crossed(cps[0]) {
                self.started = true;
                self.next_cp = 1;
                self.lap_start_tick = tick;
                return LapEvent::Started;
            }
            return LapEvent::None;
        }
        if self.next_cp < finish && crossed(cps[self.next_cp]) {
            self.next_cp += 1;
            return LapEvent::Checkpoint(self.next_cp - 1);
        }
        if self.next_cp == finish && crossed(cps[finish]) {
            let ticks = tick - self.lap_start_tick;
            let best = self.best_lap_ticks.is_none_or(|b| ticks < b);
            if best {
                self.best_lap_ticks = Some(ticks);
            }
            self.last_lap_ticks = Some(ticks);
            self.completed.push(ticks);
            self.lap += 1;
            self.started = false;
            self.next_cp = 0;
            return LapEvent::Completed { ticks, best };
        }
        LapEvent::None
    }

    /// Vuelve a la salida: el intento en curso se descarta; mejores tiempos se conservan.
    pub fn restart(&mut self) {
        self.started = false;
        self.next_cp = 0;
    }

    pub fn current_ticks(&self, tick: u64) -> u64 {
        if self.started {
            tick - self.lap_start_tick
        } else {
            0
        }
    }

    pub fn hash_into(&self, h: &mut Fnv64) {
        h.u32(self.started as u32)
            .u32(self.next_cp as u32)
            .u64(self.lap_start_tick)
            .u32(self.lap)
            .u64(self.best_lap_ticks.unwrap_or(u64::MAX))
            .u32(self.invalidated);
    }
}
