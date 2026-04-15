pub struct Colormap {
    points: Vec<[f32; 4]>,
}

impl Colormap {
    pub fn new(points: Vec<[f32; 4]>) -> Self {
        let mut pts = points;
        pts.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());
        Self { points: pts }
    }

    pub fn sample(&self, t: f32) -> [u8; 3] {
        let pts = &self.points;
        if pts.len() < 2 {
            return [0, 0, 0];
        }
        let t = t.clamp(pts.first().unwrap()[0], pts.last().unwrap()[0]);
        let idx = pts
            .windows(2)
            .position(|w| t >= w[0][0] && t <= w[1][0])
            .unwrap_or(pts.len() - 2);
        let p0 = &pts[idx.saturating_sub(1)];
        let p1 = &pts[idx];
        let p2 = &pts[(idx + 1).min(pts.len() - 1)];
        let p3 = &pts[(idx + 2).min(pts.len() - 1)];
        let seg_t0 = p1[0];
        let seg_t1 = p2[0];
        let local_t = if (seg_t1 - seg_t0).abs() < 1e-6 {
            0.0
        } else {
            (t - seg_t0) / (seg_t1 - seg_t0)
        };
        let r = catmull_rom(local_t, p0[1], p1[1], p2[1], p3[1]);
        let g = catmull_rom(local_t, p0[2], p1[2], p2[2], p3[2]);
        let b = catmull_rom(local_t, p0[3], p1[3], p2[3], p3[3]);
        [
            r.clamp(0.0, 255.0) as u8,
            g.clamp(0.0, 255.0) as u8,
            b.clamp(0.0, 255.0) as u8,
        ]
    }
}

fn catmull_rom(t: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

pub fn tonemap(raw: f32, exposure: f32) -> f32 {
    1.0 - (-exposure * raw).exp()
}
