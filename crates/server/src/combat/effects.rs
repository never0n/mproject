use mortown_protocol::skills::{Status, StatusKind};

pub struct Dot {
    pub kind: StatusKind,
    pub owner: i64,
    pub damage: f64,
    pub until: u64,
    pub next: u64,
    pub travelled: f32,
}
#[derive(Default)]
pub struct Effects {
    pub blind_until: u64,
    pub cripple_until: u64,
    pub sand_until: u64,
    pub dots: Vec<Dot>,
}
impl Effects {
    pub fn dot(&mut self, kind: StatusKind, owner: i64, damage: f64, tick: u64, seconds: u64) {
        // Refresh one instance of each effect; repeated casts do not create unbounded stacks.
        self.dots.retain(|d| d.kind != kind);
        self.dots.push(Dot {
            kind,
            owner,
            damage: damage.max(1.),
            until: tick + seconds * 20,
            next: tick + 20,
            travelled: 0.,
        });
    }
    pub fn views(&self, tick: u64) -> Vec<Status> {
        let mut statuses = vec![];
        for (kind, until) in [
            (StatusKind::Blinded, self.blind_until),
            (StatusKind::Crippled, self.cripple_until),
            (StatusKind::Quicksand, self.sand_until),
        ]
        .into_iter()
        .chain(self.dots.iter().map(|d| (d.kind, d.until)))
        {
            if until > tick {
                statuses.push(Status {
                    kind,
                    remaining_ms: (until - tick) as u32 * 50,
                });
            }
        }
        statuses
    }
    pub fn advance(&mut self, tick: u64, moved: f32) -> Vec<(i64, f64, StatusKind)> {
        let mut hits = vec![];
        for dot in &mut self.dots {
            dot.travelled += moved;
            if tick >= dot.next && tick <= dot.until {
                let factor = if dot.kind == StatusKind::Bleeding {
                    1. + f64::from((dot.travelled * 0.1).min(1.))
                } else {
                    1.
                };
                hits.push((dot.owner, (dot.damage * factor).round().max(1.), dot.kind));
                dot.next = tick + 20;
            }
        }
        self.dots.retain(|d| d.until > tick);
        hits
    }
    pub fn accuracy_multiplier(&self, tick: u64) -> f64 {
        (if self.blind_until > tick { 0.2 } else { 1. })
            * (if self.sand_until > tick { 0.7 } else { 1. })
    }
}
