use noise::{Fbm, NoiseFn, Perlin, RidgedMulti, SuperSimplex};

// noise 0.9.0 の Fbm 内部 MAX_OCTAVES = 6
const FBM_MAX_OCTAVES: usize = 6;

pub struct NebulaNoiseField {
    /// メインの密度場 (大スケール雲塊)
    density: Fbm<Perlin>,
    /// 雲内のテクスチャ (中スケール)
    detail: Fbm<SuperSimplex>,
    /// 発光ノット (HII 領域, 恒星形成域)
    emission: RidgedMulti<Perlin>,
    /// Domain Warp 用: x 方向の座標歪み
    warp_x: Fbm<Perlin>,
    /// Domain Warp 用: y 方向の座標歪み
    warp_y: Fbm<Perlin>,
    /// warp の強度 (控えめ: 崖状の輪郭を保つ)
    warp_strength: f64,
}

impl NebulaNoiseField {
    pub fn new(seed: u32, octaves: usize) -> Self {
        let octaves = octaves.min(FBM_MAX_OCTAVES);

        // 大スケール密度場: 低周波で大きな雲塊を生成 (Carina 星雲の崖状構造)
        let mut density = Fbm::<Perlin>::new(seed);
        density.octaves = octaves;
        density.frequency = 1.0;   // 低周波 = 大きな雲塊
        density.lacunarity = 2.0;
        density.persistence = 0.55; // 高め: なめらかな大スケール

        // 雲内テクスチャ: 中スケールで細部の凹凸を追加
        let mut detail = Fbm::<SuperSimplex>::new(seed.wrapping_add(1));
        detail.octaves = octaves.saturating_sub(1).max(3).min(FBM_MAX_OCTAVES);
        detail.frequency = 2.5;
        detail.lacunarity = 2.1;
        detail.persistence = 0.48;

        // 発光域: RidgedMulti で輝点/恒星形成ノットを生成
        let mut emission = RidgedMulti::<Perlin>::new(seed.wrapping_add(2));
        emission.octaves = octaves.saturating_sub(1).max(3);
        emission.frequency = 2.0;
        emission.lacunarity = 2.0;
        emission.attenuation = 2.0;

        // 単段ワープ: ゆるやかに雲の輪郭を曲げる (2段ワープは細いフィラメント原因)
        let mut warp_x = Fbm::<Perlin>::new(seed.wrapping_add(10));
        warp_x.octaves = octaves.saturating_sub(2).max(3).min(FBM_MAX_OCTAVES);
        warp_x.frequency = 0.5;  // 非常に低周波 = 大きな流れ
        warp_x.lacunarity = 2.0;
        warp_x.persistence = 0.5;

        let mut warp_y = Fbm::<Perlin>::new(seed.wrapping_add(11));
        warp_y.octaves = octaves.saturating_sub(2).max(3).min(FBM_MAX_OCTAVES);
        warp_y.frequency = 0.5;
        warp_y.lacunarity = 2.0;
        warp_y.persistence = 0.5;

        Self {
            density,
            detail,
            emission,
            warp_x,
            warp_y,
            warp_strength: 0.20, // 控えめ: 崖状の輪郭を保つ
        }
    }

    /// 正規化座標 (nx, ny) でのノイズ値を返す
    /// 単段 Domain Warping で大きな雲塊の自然な曲率を生成
    pub fn sample(&self, nx: f64, ny: f64) -> NoiseSample {
        // 単段ワープ: 大きなスケールの流れで輪郭を湾曲させる
        let wx = self.warp_x.get([nx, ny]);
        let wy = self.warp_y.get([nx + 5.2, ny + 1.3]);

        let warped_x = nx + wx * self.warp_strength;
        let warped_y = ny + wy * self.warp_strength;

        // 大スケール密度 (雲の在無を決める)
        let density = normalize(self.density.get([warped_x, warped_y]));

        // 中スケール詳細テクスチャ (雲内の凹凸)
        let detail_val = normalize(self.detail.get([warped_x * 1.8, warped_y * 1.8]));

        // 発光域 (恒星形成ノット, HII領域コア)
        let emission = normalize(self.emission.get([warped_x + 3.1, warped_y + 7.4]));

        // 合成: 密度主導 (大きな雲塊を重視)
        // density: 雲の在無を決定
        // detail: 雲内の凹凸テクスチャ
        // emission: 発光ノット
        let composite = (density * 0.65 + detail_val * 0.25 + emission * 0.10)
            .clamp(0.0, 1.0);

        NoiseSample {
            density,
            turbulence: detail_val,
            emission,
            composite,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NoiseSample {
    pub density: f64,
    pub turbulence: f64,
    pub emission: f64,
    pub composite: f64,
}

/// [-1, 1] → [0, 1] に正規化
fn normalize(v: f64) -> f64 {
    (v + 1.0) * 0.5
}

/// ガウシアンビネット: 中心からの距離に応じて減衰
pub fn vignette(nx: f64, ny: f64, strength: f64) -> f64 {
    let r2 = nx * nx + ny * ny;
    (-r2 * strength).exp()
}
